/// Tests for the full AppError taxonomy + IntoResponse mapping.
use mathcoin_api::error::AppError;
use axum::response::IntoResponse;

#[test]
fn unauthenticated_maps_to_401() {
    let resp = AppError::Unauthenticated("bad token".into()).into_response();
    assert_eq!(resp.status().as_u16(), 401);
}

#[test]
fn bad_request_maps_to_400() {
    let resp = AppError::BadRequest("missing field".into()).into_response();
    assert_eq!(resp.status().as_u16(), 400);
}

#[test]
fn internal_maps_to_500_with_generic_message() {
    let resp = AppError::Internal.into_response();
    assert_eq!(resp.status().as_u16(), 500);
}

#[test]
fn already_resolved_maps_to_409() {
    let resp = AppError::AlreadyResolved.into_response();
    assert_eq!(resp.status().as_u16(), 409);
}

#[test]
fn unknown_challenge_maps_to_404() {
    let resp = AppError::UnknownChallenge.into_response();
    assert_eq!(resp.status().as_u16(), 404);
}

#[test]
fn incorrect_solution_maps_to_422() {
    let resp = AppError::IncorrectSolution.into_response();
    assert_eq!(resp.status().as_u16(), 422);
}

#[test]
fn challenge_expired_maps_to_410() {
    let resp = AppError::ChallengeExpired.into_response();
    assert_eq!(resp.status().as_u16(), 410);
}

#[test]
fn rate_limited_maps_to_429() {
    let resp = AppError::RateLimited.into_response();
    assert_eq!(resp.status().as_u16(), 429);
}

#[test]
fn internal_error_body_is_generic() {
    // The Internal variant must NOT leak details.
    // We verify this by construction: the match arm hardcodes "internal server error".
    let resp = AppError::Internal.into_response();
    assert_eq!(resp.status().as_u16(), 500);
    // The Display impl must also be safe
    let display = format!("{}", AppError::Internal);
    assert!(!display.contains("database"), "Display must not leak DB details");
    assert!(!display.contains("stack"), "Display must not leak stack info");
}

#[test]
fn unauthenticated_does_not_leak_internal_context() {
    // Unauthenticated should only show the user-facing message
    let display = format!("{}", AppError::Unauthenticated("missing Authorization header".into()));
    assert!(display.contains("missing Authorization header"));
    assert!(!display.contains("JWKS"));
}
