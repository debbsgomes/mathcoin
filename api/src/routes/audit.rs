use axum::{extract::State, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct AuditResponse {
    pub onchain_enabled: bool,
    pub contract_address: Option<String>,
    pub chain: Option<String>,
    pub explorer: Option<String>,
    pub merkle_root: Option<String>,
    pub total_accrued_supply: i64,
    pub distribution_count: i64,
    pub last_published_at: Option<String>,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AuditResponse>, AppError> {
    let (onchain_enabled, contract_address, chain, explorer) = match &state.onchain_config {
        Some(cfg) => (
            true,
            Some(cfg.contract_address.clone()),
            Some(cfg.chain_name.clone()),
            Some(cfg.explorer_url.clone()),
        ),
        None => (false, None, None, None),
    };

    let latest: Option<(String, String)> = sqlx::query_as(
        "SELECT merkle_root, created_at::text FROM distributions
         WHERE status = 'published'
         ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    let supply: (Option<i64>,) = sqlx::query_as(
        "SELECT COALESCE(SUM(amount), 0)::BIGINT FROM earnings",
    )
    .fetch_one(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    let dist_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM distributions WHERE status = 'published'",
    )
    .fetch_one(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    let (merkle_root, last_published_at) = match latest {
        Some((root, ts)) => (Some(root), Some(ts)),
        None => (None, None),
    };

    Ok(Json(AuditResponse {
        onchain_enabled,
        contract_address,
        chain,
        explorer,
        merkle_root,
        total_accrued_supply: supply.0.unwrap_or(0),
        distribution_count: dist_count.0,
        last_published_at,
    }))
}
