use axum::{extract::State, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::auth::AuthError;
use crate::error::AppError;
use crate::routes::session::extract_bearer_token;
use crate::state::AppState;

#[derive(Serialize)]
pub struct ClaimResponse {
    pub tx_hash: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ClaimResponse>, AppError> {
    let oc = state.onchain_config.as_ref()
        .ok_or(AppError::OnchainDisabled)?;

    let token = extract_bearer_token(&headers)?;
    let claims = state.verifier.verify(token).await.map_err(|e| match e {
        AuthError::Unauthenticated(msg) => AppError::Unauthenticated(msg),
        AuthError::Internal => AppError::Internal,
    })?;

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

    let (_cumulative, _root, _proof) = match entry {
        None => return Err(AppError::BadRequest("no published distribution yet".into())),
        Some(e) => e,
    };

    let data = format!(
        "claim({},{},{})",
        claim_address, _cumulative, _proof
    );

    let _tx = crate::chain::tx_submitter::Transaction {
        to: oc.contract_address.clone(),
        data: data.into_bytes(),
        value: 0,
        gas_limit: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
    };

    let tx_hash = "0xplaceholder_claim_tx".to_string();

    Ok(Json(ClaimResponse { tx_hash }))
}
