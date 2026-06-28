use axum::{extract::State, Json};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::challenge::generate_challenge;

use crate::auth::AuthError;
use crate::error::AppError;
use crate::routes::session::extract_bearer_token;
use crate::state::AppState;

/// Fixed difficulty constant for Phase 2. Replaced by retarget controller in Phase 4.
const PHASE2_DIFFICULTY: u32 = 3;

#[derive(Serialize)]
pub struct ChallengeResponse {
    pub challenge_id: Uuid,
    pub problem: String,
    pub difficulty: u32,
    pub reward: i64,
    pub expires_at: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ChallengeResponse>, AppError> {
    let token = extract_bearer_token(&headers)?;
    let claims = state.verifier.verify(token).await.map_err(|e| match e {
        AuthError::Unauthenticated(msg) => AppError::Unauthenticated(msg),
        AuthError::Internal => AppError::Internal,
    })?;

    // Look up user id
    let user_id: (i64,) = sqlx::query_as("SELECT id FROM users WHERE provider_sub = $1")
        .bind(&claims.sub)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("failed to look up user: {e}");
            AppError::Internal
        })?
        .ok_or_else(|| AppError::Unauthenticated("user not found".into()))?;

    // Generate challenge at fixed difficulty
    let ttl_seconds = 60;
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(ttl_seconds);

    let (problem, solution, reward) = {
        let mut rng = rand::thread_rng();
        generate_challenge(PHASE2_DIFFICULTY, &mut rng)
    };

    let challenge_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO challenges (id, user_id, problem, solution, difficulty, reward, status, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'PENDING', $7)",
    )
    .bind(challenge_id)
    .bind(user_id.0)
    .bind(&problem)
    .bind(solution)
    .bind(PHASE2_DIFFICULTY as i16)
    .bind(reward)
    .bind(expires_at)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("failed to insert challenge: {e}");
        AppError::Internal
    })?;

    Ok(Json(ChallengeResponse {
        challenge_id,
        problem,
        difficulty: PHASE2_DIFFICULTY,
        reward,
        expires_at: expires_at.to_rfc3339(),
    }))
}
