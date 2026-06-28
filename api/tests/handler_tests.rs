/// Handler integration tests: POST /api/session and GET /api/me.
/// Uses a shared test Postgres + mock AuthVerifier.
/// Each test clears its own data.
use axum::body::Body;
use axum::http::{self, Request};
use mathcoin_api::auth::{AuthVerifier, MockVerifier};
use mathcoin_api::state::AppState;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://mathcoin:mathcoin@localhost:5432/mathcoin_test".into());
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .unwrap()
}

async fn clean_db(pool: &PgPool) {
    sqlx::query("DELETE FROM earnings").execute(pool).await.unwrap();
    sqlx::query("DELETE FROM challenges").execute(pool).await.unwrap();
    sqlx::query("DELETE FROM users").execute(pool).await.unwrap();
}

fn test_app(pool: &PgPool, verifier: Arc<dyn AuthVerifier>) -> axum::Router {
    use axum::routing::{get, post};
    let state = Arc::new(AppState {
        db: pool.clone(),
        verifier,
    });
    axum::Router::new()
        .route("/api/session", post(mathcoin_api::routes::session::handler))
        .route("/api/me", get(mathcoin_api::routes::me::handler))
        .route("/api/challenge", get(mathcoin_api::routes::challenge::handler))
        .with_state(state)
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

// ---- POST /api/session ----

#[tokio::test]
async fn session_valid_token_upserts_user() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting(
        "sub-001".into(),
        "deb@example.com".into(),
    ));
    let app = test_app(&pool, verifier);

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/session")
                .header("Authorization", bearer("valid-token"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value =
        serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await.unwrap())
            .unwrap();
    assert_eq!(body["email"], "deb@example.com");
    assert!(body["user_id"].is_number());
    assert_eq!(body["balance"], 0);
    assert!(body["claim_address"].is_null());
}

#[tokio::test]
async fn session_idempotent_on_repeat() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting(
        "sub-002".into(),
        "deb@example.com".into(),
    ));
    let app = test_app(&pool, verifier);

    let r1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/session")
                .header("Authorization", bearer("t1"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let r1_status = r1.status();
    let _ = axum::body::to_bytes(r1.into_body(), 1024).await.unwrap();
    assert_eq!(r1_status, 200);

    let r2 = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/session")
                .header("Authorization", bearer("t2"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r2.status(), 200);

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE provider_sub = 'sub-002'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1, "should only have one row for sub-002");
}

#[tokio::test]
async fn session_missing_auth_header_returns_401() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::rejecting());
    let app = test_app(&pool, verifier);

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    let body: serde_json::Value =
        serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await.unwrap())
            .unwrap();
    assert_eq!(body["error"], "unauthenticated");
}

#[tokio::test]
async fn session_invalid_token_returns_401() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::rejecting());
    let app = test_app(&pool, verifier);

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/session")
                .header("Authorization", bearer("bad-token"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn session_identity_from_token_not_body() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting(
        "sub-jwt-003".into(),
        "jwt@example.com".into(),
    ));
    let app = test_app(&pool, verifier);

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/session")
                .header("Authorization", bearer("valid"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"sub": "hijacked-sub", "email": "evil@hack.com"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value =
        serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await.unwrap())
            .unwrap();
    assert_eq!(body["email"], "jwt@example.com");
}

// ---- GET /api/me ----

#[tokio::test]
async fn me_returns_user_identity() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting(
        "sub-me-001".into(),
        "me@example.com".into(),
    ));
    let app = test_app(&pool, verifier);

    let session = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/session")
                .header("Authorization", bearer("t1"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(session.status(), 200);

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/api/me")
                .header("Authorization", bearer("t2"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value =
        serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await.unwrap())
            .unwrap();
    assert_eq!(body["email"], "me@example.com");
    assert!(body["user_id"].is_number());
    assert_eq!(body["balance"], 0);
    assert!(body["claim_address"].is_null());
}

#[tokio::test]
async fn me_missing_auth_returns_401() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::rejecting());
    let app = test_app(&pool, verifier);

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/api/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
}

// ---- GET /api/challenge ----

#[tokio::test]
async fn challenge_creates_pending_row_and_returns_public_fields() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting(
        "sub-ch-001".into(),
        "ch@example.com".into(),
    ));
    let app = test_app(&pool, verifier.clone());
    let s = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/session")
                .header("Authorization", bearer("t1"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(s.status(), 200);

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/api/challenge")
                .header("Authorization", bearer("t2"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value =
        serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await.unwrap())
            .unwrap();

    assert!(body["challenge_id"].is_string(), "challenge_id missing");
    assert!(body["problem"].is_string(), "problem missing");
    assert!(body["difficulty"].is_number(), "difficulty missing");
    assert!(body["reward"].is_number(), "reward missing");
    assert!(body["expires_at"].is_string(), "expires_at missing");
    assert!(body.get("solution").is_none(), "solution leaked in response");

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM challenges WHERE status = 'PENDING'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 1, "should have exactly one PENDING challenge");
}

#[tokio::test]
async fn challenge_unauth_returns_401() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::rejecting());
    let app = test_app(&pool, verifier);

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/api/challenge")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
}
