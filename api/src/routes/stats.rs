use axum::{extract::State, Json};
use serde::Serialize;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct StatsResponse {
    pub current_difficulty: u32,
    pub mints_last_60s: f64,
    pub target_rate_per_60s: f64,
    pub total_accrued_supply: i64,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<StatsResponse>, AppError> {
    let current_difficulty = state.difficulty.load(Ordering::Relaxed);

    let mints_last_60s = {
        let mut stats = state.mint_stats.lock().await;
        let now = state.clock.now();
        stats.rate(state.retarget_config.window, now)
    };

    let total_accrued_supply: (Option<i64>,) = sqlx::query_as(
        "SELECT COALESCE(SUM(amount), 0)::BIGINT FROM earnings",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or((Some(0),));

    Ok(Json(StatsResponse {
        current_difficulty,
        mints_last_60s,
        target_rate_per_60s: state.retarget_config.target_rate,
        total_accrued_supply: total_accrued_supply.0.unwrap_or(0),
    }))
}
