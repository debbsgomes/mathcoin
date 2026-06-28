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

    let user_id: (i64,) = sqlx::query_as("SELECT id FROM users WHERE provider_sub = $1")
        .bind(&claims.sub)
        .fetch_one(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;

    // Step 1: check challenge state and expiry in one query
    let row: Option<(String, i64, i64, bool)> = sqlx::query_as(
        "SELECT status, solution, reward, expires_at <= now()
         FROM challenges WHERE id = $1",
    )
    .bind(body.challenge_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    let (status, solution, reward, expired) = match row {
        None => return Err(AppError::UnknownChallenge),
        Some(r) => r,
    };

    if status != "PENDING" {
        return Err(AppError::AlreadyResolved);
    }

    if expired {
        sqlx::query(
            "UPDATE challenges SET status = 'EXPIRED', resolved_at = now()
             WHERE id = $1 AND status = 'PENDING'",
        )
        .bind(body.challenge_id)
        .execute(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
        return Err(AppError::ChallengeExpired);
    }

    // Step 3: check answer
    if body.answer != solution {
        sqlx::query(
            "UPDATE challenges SET status = 'EXPIRED', resolved_at = now()
             WHERE id = $1 AND status = 'PENDING'",
        )
        .bind(body.challenge_id)
        .execute(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
        return Err(AppError::IncorrectSolution);
    }

    // Step 4: mark CLAIMED and get reward (CAS — only one caller succeeds)
    let claimed: Option<(i64,)> = sqlx::query_as(
        "UPDATE challenges SET status = 'CLAIMED', resolved_at = now()
         WHERE id = $1 AND status = 'PENDING'
         RETURNING reward",
    )
    .bind(body.challenge_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    let reward = match claimed {
        None => return Err(AppError::AlreadyResolved),
        Some((r,)) => r,
    };

    // Step 5: credit earnings (UNIQUE constraint is defense-in-depth)
    sqlx::query(
        "INSERT INTO earnings (user_id, challenge_id, amount) VALUES ($1, $2, $3)",
    )
    .bind(user_id.0)
    .bind(body.challenge_id)
    .bind(reward)
    .execute(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    // Step 6: compute balance
    let balance: (Option<i64>,) = sqlx::query_as(
        "SELECT SUM(amount)::BIGINT FROM earnings WHERE user_id = $1",
    )
    .bind(user_id.0)
    .fetch_one(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    Ok(Json(MintResponse {
        status: "CLAIMED".into(),
        reward,
        balance: balance.0.unwrap_or(0),
    }))
}
