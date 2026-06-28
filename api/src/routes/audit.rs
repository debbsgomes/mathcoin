use axum::{extract::State, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct AuditResponse {
    pub contract_address: String,
    pub chain: String,
    pub explorer: String,
    pub merkle_root: Option<String>,
    pub total_accrued_supply: i64,
    pub distribution_count: i64,
    pub last_published_at: Option<String>,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AuditResponse>, AppError> {
    let contract_address = std::env::var("CONTRACT_ADDRESS")
        .unwrap_or_else(|_| "0x0000000000000000000000000000000000000000".into());

    let chain = std::env::var("CHAIN_NAME").unwrap_or_else(|_| "base_sepolia".into());
    let explorer = std::env::var("EXPLORER_URL")
        .unwrap_or_else(|_| "https://sepolia.basescan.org".into());

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
    .unwrap_or((Some(0),));

    let dist_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM distributions WHERE status = 'published'",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or((0,));

    let (merkle_root, last_published_at) = match latest {
        Some((root, ts)) => (Some(root), Some(ts)),
        None => (None, None),
    };

    Ok(Json(AuditResponse {
        contract_address,
        chain,
        explorer,
        merkle_root,
        total_accrued_supply: supply.0.unwrap_or(0),
        distribution_count: dist_count.0,
        last_published_at,
    }))
}
