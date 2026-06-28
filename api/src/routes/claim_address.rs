use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::AuthError;
use crate::error::AppError;
use crate::routes::session::extract_bearer_token;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ClaimAddressRequest {
    pub address: String,
}

#[derive(Serialize)]
pub struct ClaimAddressResponse {
    pub claim_address: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<ClaimAddressRequest>,
) -> Result<Json<ClaimAddressResponse>, AppError> {
    let token = extract_bearer_token(&headers)?;
    let claims = state.verifier.verify(token).await.map_err(|e| match e {
        AuthError::Unauthenticated(msg) => AppError::Unauthenticated(msg),
        AuthError::Internal => AppError::Internal,
    })?;

    // Validate EIP-55 checksum
    let addr: alloy_primitives::Address = body
        .address
        .parse()
        .map_err(|_| AppError::BadRequest("invalid Ethereum address format".into()))?;

    let checksummed = addr.to_checksum(None);
    if checksummed != body.address {
        return Err(AppError::BadRequest("invalid Ethereum address checksum".into()));
    }

    let address_str = addr.to_checksum(None);

    let result = sqlx::query(
        "UPDATE users SET claim_address = $1 WHERE provider_sub = $2 AND claim_address IS NULL",
    )
    .bind(&address_str)
    .bind(&claims.sub)
    .execute(&state.db)
    .await
    .map_err(|e| {
        // UNIQUE constraint violation → address already in use
        if e.as_database_error()
            .map(|d| d.code().as_deref() == Some("23505"))
            .unwrap_or(false)
        {
            AppError::BadRequest("address already in use".into())
        } else {
            tracing::error!("failed to set claim_address: {e}");
            AppError::Internal
        }
    })?;

    if result.rows_affected() == 0 {
        // Either user not found or already has a claim_address
        // Check which
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT email FROM users WHERE provider_sub = $1")
                .bind(&claims.sub)
                .fetch_optional(&state.db)
                .await
                .map_err(|_| AppError::Internal)?;

        if existing.is_none() {
            return Err(AppError::Unauthenticated("user not found".into()));
        }
        // User exists but already has an address or address unchanged
        return Err(AppError::BadRequest("claim address already set".into()));
    }

    Ok(Json(ClaimAddressResponse {
        claim_address: address_str,
    }))
}
