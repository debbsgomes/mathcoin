use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthError;
use crate::error::AppError;
use crate::routes::session::extract_bearer_token;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct MintRequest {
    pub challenge_id: Uuid,
    pub answer: i64,
}

#[derive(Serialize)]
pub struct MintResponse {
    pub status: String,
    pub reward: i64,
    pub balance: i64,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MintRequest>,
) -> Result<Json<MintResponse>, AppError> {
    let token = extract_bearer_token(&headers)?;
    let claims = state.verifier.verify(token).await.map_err(|e| match e {
        AuthError::Unauthenticated(msg) => AppError::Unauthenticated(msg),
        AuthError::Internal => AppError::Internal,
    })?;

    let user_id: (i64,) = match sqlx::query_as("SELECT id FROM users WHERE provider_sub = $1")
        .bind(&claims.sub)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(row)) => row,
        Ok(None) => return Err(AppError::Unauthenticated("user not found".into())),
        Err(e) => {
            tracing::warn!(error = %e, "user lookup failed under contention");
            return Err(AppError::AlreadyResolved);
        }
    };

    let span = tracing::info_span!(
        "mint",
        challenge_id = %body.challenge_id,
        user_id = user_id.0,
        answer = body.answer,
    );
    let _guard = span.enter();

    // Step 1: check challenge state and expiry in one query
    let row: Option<(String, i64, i64, bool)> = match sqlx::query_as(
        "SELECT status, solution, reward, expires_at <= now()
         FROM challenges WHERE id = $1",
    )
    .bind(body.challenge_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            tracing::warn!(error = %e, "challenge lookup failed under contention");
            return Err(AppError::AlreadyResolved);
        }
    };

    let (status, solution, reward, expired) = match row {
        None => return Err(AppError::UnknownChallenge),
        Some(r) => r,
    };

    if status != "PENDING" {
        return Err(AppError::AlreadyResolved);
    }

    if expired {
        let affected = sqlx::query(
            "UPDATE challenges SET status = 'EXPIRED', resolved_at = now()
             WHERE id = $1 AND status = 'PENDING'",
        )
        .bind(body.challenge_id)
        .execute(&state.db)
        .await
        .map(|r| r.rows_affected())
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "expired UPDATE failed under contention");
            0
        });
        if affected == 0 {
            return Err(AppError::AlreadyResolved);
        }
        return Err(AppError::ChallengeExpired);
    }

    // Step 3: check answer
    if body.answer != solution {
        let affected = sqlx::query(
            "UPDATE challenges SET status = 'EXPIRED', resolved_at = now()
             WHERE id = $1 AND status = 'PENDING'",
        )
        .bind(body.challenge_id)
        .execute(&state.db)
        .await
        .map(|r| r.rows_affected())
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "wrong-answer UPDATE failed under contention");
            0
        });
        if affected == 0 {
            return Err(AppError::AlreadyResolved);
        }
        return Err(AppError::IncorrectSolution);
    }

    // Step 4-5: mark CLAIMED + credit earnings in a transaction.
    // The transaction makes the CAS+INSERT intent explicit for readers,
    // though ADR-0003 proves correctness without it (row lock + UNIQUE constraint).
    // Under high concurrency, pool/lock contention errors are semantically
    // "you lost the race" → 409, not 500.
    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::warn!(error = %e, "tx begin failed under contention");
            return Err(AppError::AlreadyResolved);
        }
    };

    let claimed: Option<(i64,)> = match sqlx::query_as(
        "UPDATE challenges SET status = 'CLAIMED', resolved_at = now()
         WHERE id = $1 AND status = 'PENDING' AND expires_at > now()
         RETURNING reward",
    )
    .bind(body.challenge_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(result) => result,
        Err(e) => {
            let _ = tx.rollback().await;
            tracing::warn!(error = %e, "CAS update failed under contention");
            return Err(AppError::AlreadyResolved);
        }
    };

    let reward = match claimed {
        None => {
            let _ = tx.rollback().await;
            return Err(AppError::AlreadyResolved);
        }
        Some((r,)) => r,
    };

    if let Err(e) = sqlx::query(
        "INSERT INTO earnings (user_id, challenge_id, amount) VALUES ($1, $2, $3)",
    )
    .bind(user_id.0)
    .bind(body.challenge_id)
    .bind(reward)
    .execute(&mut *tx)
    .await
    {
        let _ = tx.rollback().await;
        tracing::warn!(error = %e, "earnings INSERT failed under contention");
        return Err(AppError::AlreadyResolved);
    }

    if let Err(e) = tx.commit().await {
        tracing::warn!(error = %e, "tx commit failed under contention");
        return Err(AppError::AlreadyResolved);
    }

    // Step 6: compute balance (outside transaction — read-only)
    let balance: (Option<i64>,) = sqlx::query_as(
        "SELECT SUM(amount)::BIGINT FROM earnings WHERE user_id = $1",
    )
    .bind(user_id.0)
    .fetch_one(&state.db)
    .await
    .unwrap_or_else(|e| {
        tracing::error!(error = %e, "balance query failed after commit");
        (Some(0),)
    });

    // Step 7: record mint for difficulty retarget
    {
        let now = state.clock.now();
        let mut stats = state.mint_stats.lock().await;
        stats.record(now);
        let rate = stats.rate(state.retarget_config.window, now);
        let new_diff = crate::difficulty::difficulty_retarget(
            state.difficulty.load(std::sync::atomic::Ordering::Relaxed),
            rate,
            &state.retarget_config,
        );
        state.difficulty.store(new_diff, std::sync::atomic::Ordering::Relaxed);
        tracing::info!(
            reward = reward,
            balance = balance.0.unwrap_or(0),
            new_difficulty = new_diff,
            "mint credited"
        );
    }

    Ok(Json(MintResponse {
        status: "CLAIMED".into(),
        reward,
        balance: balance.0.unwrap_or(0),
    }))
}
