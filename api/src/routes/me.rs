use axum::{extract::State, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::error::AppError;
use crate::routes::session::extract_bearer_token;
use crate::state::AppState;

#[derive(Serialize)]
pub struct MeResponse {
    pub user_id: i64,
    pub email: String,
    pub balance: i64,
    pub claim_address: Option<String>,
    pub claimed_onchain: Option<i64>,
    pub claimable_onchain: Option<i64>,
    pub total_mined: i64,
    pub current_difficulty: i32,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<MeResponse>, AppError> {
    let token = extract_bearer_token(&headers)?;
    let claims = state.verifier.verify(token).await.map_err(|e| match e {
        crate::auth::AuthError::Unauthenticated(msg) => AppError::Unauthenticated(msg),
        crate::auth::AuthError::Internal => AppError::Internal,
    })?;

    let row: (i64, String, Option<i64>, Option<i64>, Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT u.id, u.email,
                COALESCE(SUM(e.amount), 0)::BIGINT,
                COUNT(e.id)::BIGINT,
                u.claim_address,
                u.claimed_onchain
         FROM users u
         LEFT JOIN earnings e ON e.user_id = u.id
         WHERE u.provider_sub = $1
         GROUP BY u.id, u.email, u.claim_address, u.claimed_onchain",
    )
    .bind(&claims.sub)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("failed to look up user: {e}");
        AppError::Internal
    })?
    .ok_or_else(|| AppError::Unauthenticated("user not found".into()))?;

    let (user_id, email, balance, total_mined, claim_address, claimed_onchain) = row;

    // Published-only: latest cumulative from published distributions only
    let (claimable_onchain, should_flag) = if let Some(ref addr) = claim_address {
        let published_cumulative: Option<(Option<i64>,)> = sqlx::query_as(
            "SELECT de.cumulative_amount
             FROM distribution_entries de
             JOIN distributions d ON d.id = de.distribution_id
             WHERE d.status = 'published' AND de.wallet_address = $1
             ORDER BY d.created_at DESC LIMIT 1",
        )
        .bind(addr)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("failed to query published cumulative: {e}");
            AppError::Internal
        })?;

        match published_cumulative {
            None => (None, false),
            Some((Some(cumulative),)) => {
                let claimed = claimed_onchain.unwrap_or(0);
                if cumulative < claimed {
                    // INVARIANT VIOLATION: negative claimable → alert
                    tracing::error!(
                        user_id = user_id,
                        claim_address = %addr,
                        latest_published_cumulative = cumulative,
                        claimed_onchain = claimed,
                        "NEGATIVE CLAIMABLE — cache corruption detected"
                    );
                    (Some(0), true)
                } else {
                    (Some(cumulative - claimed), false)
                }
            }
            Some((None,)) => (None, false),
        }
    } else {
        (None, false)
    };

    if should_flag {
        // Flag address for reconciliation (deferred to indexer — Phase 5.9)
        tracing::warn!(user_id = user_id, "address flagged for reconciliation");
    }

    Ok(Json(MeResponse {
        user_id,
        email,
        balance: balance.unwrap_or(0),
        claim_address,
        claimed_onchain,
        claimable_onchain,
        total_mined: total_mined.unwrap_or(0),
        current_difficulty: state.difficulty.load(std::sync::atomic::Ordering::Relaxed) as i32,
    }))
}
