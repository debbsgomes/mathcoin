use axum::{extract::State, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::auth::AuthError;
use crate::error::AppError;
use crate::routes::session::extract_bearer_token;
use crate::state::AppState;

#[derive(Serialize)]
pub struct ClaimDataResponse {
    pub contract_address: String,
    pub account: String,
    pub cumulative_amount: i64,
    pub merkle_root: String,
    pub proof: serde_json::Value,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ClaimDataResponse>, AppError> {
    let token = extract_bearer_token(&headers)?;
    let claims = state.verifier.verify(token).await.map_err(|e| match e {
        AuthError::Unauthenticated(msg) => AppError::Unauthenticated(msg),
        AuthError::Internal => AppError::Internal,
    })?;

    // Get user's claim_address
    let row: (Option<String>,) = sqlx::query_as(
        "SELECT claim_address FROM users WHERE provider_sub = $1",
    )
    .bind(&claims.sub)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::Internal)?
    .ok_or_else(|| AppError::Unauthenticated("user not found".into()))?;

    let claim_address = row.0.ok_or_else(|| {
        AppError::BadRequest("no claim address set".into())
    })?;

    // Published-only: join distributions and filter status='published'
    let entry: Option<(i64, String, serde_json::Value)> = sqlx::query_as(
        "SELECT de.cumulative_amount, d.merkle_root, de.proof
         FROM distribution_entries de
         JOIN distributions d ON d.id = de.distribution_id
         WHERE d.status = 'published' AND de.wallet_address = $1
         ORDER BY d.created_at DESC LIMIT 1",
    )
    .bind(&claim_address)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    match entry {
        None => Err(AppError::BadRequest(
            "no distribution yet — available after the next sync".into(),
        )),
        Some((cumulative, root, proof)) => Ok(Json(ClaimDataResponse {
            contract_address: std::env::var("CONTRACT_ADDRESS")
                .unwrap_or_else(|_| "0x0000000000000000000000000000000000000000".into()),
            account: claim_address,
            cumulative_amount: cumulative,
            merkle_root: root,
            proof,
        })),
    }
}
