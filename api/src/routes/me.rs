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

    // Look up user by provider_sub
    let row: (i64, String) = sqlx::query_as(
        "SELECT id, email FROM users WHERE provider_sub = $1",
    )
    .bind(&claims.sub)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("failed to look up user: {e}");
        AppError::Internal
    })?
    .ok_or_else(|| AppError::Unauthenticated("user not found".into()))?;

    Ok(Json(MeResponse {
        user_id: row.0,
        email: row.1,
        balance: 0,
        claim_address: None,
        total_mined: 0,
        current_difficulty: 1,
    }))
}
