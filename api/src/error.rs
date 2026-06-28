use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("unauthenticated: {0}")]
    Unauthenticated(String),
    #[error("internal error")]
    Internal,
    #[error("challenge already resolved")]
    AlreadyResolved,
    #[error("unknown challenge")]
    UnknownChallenge,
    #[error("incorrect solution")]
    IncorrectSolution,
    #[error("challenge expired")]
    ChallengeExpired,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::Unauthenticated(msg) => {
                (StatusCode::UNAUTHORIZED, "unauthenticated", msg)
            }
            AppError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "internal server error".to_string(),
            ),
            AppError::AlreadyResolved => (
                StatusCode::CONFLICT,
                "challenge_already_resolved",
                "challenge already resolved".to_string(),
            ),
            AppError::UnknownChallenge => (
                StatusCode::NOT_FOUND,
                "unknown_challenge",
                "challenge not found".to_string(),
            ),
            AppError::IncorrectSolution => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "incorrect_solution",
                "incorrect solution".to_string(),
            ),
            AppError::ChallengeExpired => (
                StatusCode::GONE,
                "challenge_expired",
                "challenge expired".to_string(),
            ),
        };
        let body = ErrorBody {
            error: code.to_string(),
            message,
        };
        (status, Json(body)).into_response()
    }
}
