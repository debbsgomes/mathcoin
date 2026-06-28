use axum::{extract::State, http::StatusCode, Json};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::{AuthVerifier, JwtClaims};
use crate::error::AppError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct SessionResponse {
    pub user_id: i64,
    pub email: String,
    pub balance: i64,
    pub claim_address: Option<String>,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<SessionResponse>, AppError> {
    let token = extract_bearer_token(&headers)?;
    let claims = state.verifier.verify(token).await.map_err(|e| match e {
        crate::auth::AuthError::Unauthenticated(msg) => AppError::Unauthenticated(msg),
        crate::auth::AuthError::Internal => AppError::Internal,
    })?;

    // Upsert user — idempotent on provider_sub
    let user_id: (i64,) = sqlx::query_as(
        "INSERT INTO users (provider_sub, email) VALUES ($1, $2)
         ON CONFLICT (provider_sub) DO UPDATE SET email = EXCLUDED.email
         RETURNING id",
    )
    .bind(&claims.sub)
    .bind(&claims.email)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("failed to upsert user: {e}");
        AppError::Internal
    })?;

    Ok(Json(SessionResponse {
        user_id: user_id.0,
        email: claims.email,
        balance: 0,
        claim_address: None,
    }))
}

pub fn extract_bearer_token(headers: &axum::http::HeaderMap) -> Result<&str, AppError> {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .ok_or_else(|| AppError::Unauthenticated("missing Authorization header".into()))?
        .to_str()
        .map_err(|_| AppError::Unauthenticated("invalid Authorization header".into()))?;

    auth.strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthenticated("Authorization header must be Bearer token".into()))
}
