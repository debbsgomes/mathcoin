/// Handler integration tests: POST /api/session, GET /api/me, GET /api/challenge, POST /api/mint.
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
        .route("/api/mint", post(mathcoin_api::routes::mint::handler))
        .with_state(state)
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

// ---- helpers ----

async fn create_user(app: &axum::Router) -> i64 {
    let r = app
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
    let body: serde_json::Value =
        serde_json::from_slice(&axum::body::to_bytes(r.into_body(), 1024).await.unwrap())
            .unwrap();
    body["user_id"].as_i64().unwrap()
}

async fn create_test_challenge(pool: &PgPool, user_id: i64) -> uuid::Uuid {
    let cid = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO challenges (id, user_id, problem, solution, difficulty, reward, status, expires_at)
         VALUES ($1, $2, '40 + 2', 42, 3, 20, 'PENDING', now() + INTERVAL '5 minutes')",
    )
    .bind(cid)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
    cid
}

async fn create_expired_challenge(pool: &PgPool, user_id: i64) -> uuid::Uuid {
    let cid = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO challenges (id, user_id, problem, solution, difficulty, reward, status, expires_at)
         VALUES ($1, $2, '1 + 1', 2, 1, 5, 'PENDING', now() - INTERVAL '1 minute')",
    )
    .bind(cid)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
    cid
}

// ---- POST /api/session ----

#[tokio::test]
async fn session_valid_token_upserts_user() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-001".into(), "deb@example.com".into()));
    let app = test_app(&pool, verifier);
    let response = app.oneshot(Request::builder().method(http::Method::POST).uri("/api/session").header("Authorization", bearer("valid-token")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(body["email"], "deb@example.com");
    assert!(body["user_id"].is_number());
    assert_eq!(body["balance"], 0);
    assert!(body["claim_address"].is_null());
}

#[tokio::test]
async fn session_idempotent_on_repeat() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-002".into(), "deb@example.com".into()));
    let app = test_app(&pool, verifier);
    let r1 = app.clone().oneshot(Request::builder().method(http::Method::POST).uri("/api/session").header("Authorization", bearer("t1")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(r1.status(), 200);
    let r2 = app.oneshot(Request::builder().method(http::Method::POST).uri("/api/session").header("Authorization", bearer("t2")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(r2.status(), 200);
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE provider_sub = 'sub-002'").fetch_one(&pool).await.unwrap();
    assert_eq!(count.0, 1, "should only have one row for sub-002");
}

#[tokio::test]
async fn session_missing_auth_header_returns_401() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::rejecting());
    let app = test_app(&pool, verifier);
    let response = app.oneshot(Request::builder().method(http::Method::POST).uri("/api/session").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), 401);
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(body["error"], "unauthenticated");
}

#[tokio::test]
async fn session_invalid_token_returns_401() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::rejecting());
    let app = test_app(&pool, verifier);
    let response = app.oneshot(Request::builder().method(http::Method::POST).uri("/api/session").header("Authorization", bearer("bad-token")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn session_identity_from_token_not_body() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-jwt-003".into(), "jwt@example.com".into()));
    let app = test_app(&pool, verifier);
    let response = app.oneshot(Request::builder().method(http::Method::POST).uri("/api/session").header("Authorization", bearer("valid")).header("Content-Type", "application/json").body(Body::from(r#"{"sub": "hijacked-sub", "email": "evil@hack.com"}"#)).unwrap()).await.unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(body["email"], "jwt@example.com");
}

// ---- GET /api/me ----

#[tokio::test]
async fn me_returns_user_identity() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-me-001".into(), "me@example.com".into()));
    let app = test_app(&pool, verifier);
    app.clone().oneshot(Request::builder().method(http::Method::POST).uri("/api/session").header("Authorization", bearer("t1")).body(Body::empty()).unwrap()).await.unwrap();
    let response = app.oneshot(Request::builder().method(http::Method::GET).uri("/api/me").header("Authorization", bearer("t2")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(body["email"], "me@example.com");
    assert!(body["user_id"].is_number());
}

#[tokio::test]
async fn me_missing_auth_returns_401() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::rejecting());
    let app = test_app(&pool, verifier);
    let response = app.oneshot(Request::builder().method(http::Method::GET).uri("/api/me").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn me_balance_zero_with_no_earnings() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-me-bal0".into(), "bal0@example.com".into()));
    let app = test_app(&pool, verifier);
    create_user(&app).await;
    let response = app.oneshot(Request::builder().method(http::Method::GET).uri("/api/me").header("Authorization", bearer("t1")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(body["balance"], 0);
    assert_eq!(body["total_mined"], 0);
}

#[tokio::test]
async fn me_balance_reflects_earnings_sum() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-me-bal1".into(), "bal1@example.com".into()));
    let app = test_app(&pool, verifier.clone());
    let uid = create_user(&app).await;
    let c1 = create_test_challenge(&pool, uid).await;
    let c2 = create_test_challenge(&pool, uid).await;
    sqlx::query("INSERT INTO earnings (user_id, challenge_id, amount) VALUES ($1, $2, 20)")
        .bind(uid).bind(c1).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO earnings (user_id, challenge_id, amount) VALUES ($1, $2, 30)")
        .bind(uid).bind(c2).execute(&pool).await.unwrap();
    let response = app.oneshot(Request::builder().method(http::Method::GET).uri("/api/me").header("Authorization", bearer("t1")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(body["balance"], 50);
    assert_eq!(body["total_mined"], 2);
}

#[tokio::test]
async fn me_e2e_challenge_mint_balance_roundtrip() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-me-e2e".into(), "e2e@example.com".into()));
    let app = test_app(&pool, verifier.clone());
    create_user(&app).await;

    let r = app.clone().oneshot(Request::builder().method(http::Method::GET).uri("/api/challenge").header("Authorization", bearer("t1")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(r.status(), 200);
    let ch: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(r.into_body(), 1024).await.unwrap()).unwrap();
    let cid = ch["challenge_id"].as_str().unwrap();
    let problem = ch["problem"].as_str().unwrap();
    let answer = evaluate_problem(problem);

    let r = app.clone().oneshot(Request::builder().method(http::Method::POST).uri("/api/mint").header("Authorization", bearer("t2")).header("Content-Type", "application/json").body(Body::from(format!(r#"{{"challenge_id":"{cid}","answer":{answer}}}"#))).unwrap()).await.unwrap();
    assert_eq!(r.status(), 200);

    let r = app.oneshot(Request::builder().method(http::Method::GET).uri("/api/me").header("Authorization", bearer("t3")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(r.status(), 200);
    let me: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(r.into_body(), 1024).await.unwrap()).unwrap();
    assert!(me["balance"].as_i64().unwrap() > 0, "balance should be > 0 after mint, got {}", me["balance"]);
    assert_eq!(me["total_mined"], 1);
    assert!(me["claim_address"].is_null());
}

fn evaluate_problem(problem: &str) -> i64 {
    let p = problem.trim();
    if p.contains(" mod ") {
        let parts: Vec<&str> = p.trim_matches(|c| c == '(' || c == ')').split(") mod ").collect();
        let expr = parts[0].to_string();
        let modulo: i64 = parts.get(1).unwrap_or(&"").parse().unwrap_or(1);
        return evaluate_simple(&expr) % modulo;
    }
    let clean = p.replace('(', "").replace(')', "");
    evaluate_simple(&clean)
}

fn evaluate_simple(expr: &str) -> i64 {
    let tokens: Vec<&str> = expr.split_whitespace().collect();
    if tokens.len() == 3 {
        let a: i64 = tokens[0].parse().unwrap();
        let b: i64 = tokens[2].parse().unwrap();
        match tokens[1] {
            "+" => a + b,
            "−" | "-" => a - b,
            "×" | "*" => a * b,
            _ => 0,
        }
    } else if tokens.len() == 5 {
        let a: i64 = tokens[0].parse().unwrap();
        let b: i64 = tokens[2].parse().unwrap();
        let c: i64 = tokens[4].parse().unwrap();
        let left = match tokens[1] { "×" | "*" => a * b, _ => a };
        match tokens[3] {
            "×" | "*" => left * c,
            "+" => left + c,
            "−" | "-" => left - c,
            _ => left,
        }
    } else if tokens.len() == 7 {
        let a: i64 = tokens[0].parse().unwrap();
        let b: i64 = tokens[2].parse().unwrap();
        let c: i64 = tokens[4].parse().unwrap();
        let d: i64 = tokens[6].parse().unwrap();
        let left = match tokens[1] { "×" | "*" => a * b, _ => a };
        let right = match tokens[5] { "×" | "*" => c * d, _ => c };
        match tokens[3] {
            "−" | "-" => left - right,
            "+" => left + right,
            _ => left,
        }
    } else {
        0
    }
}

// ---- GET /api/challenge ----

#[tokio::test]
async fn challenge_creates_pending_row_and_returns_public_fields() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-ch-001".into(), "ch@example.com".into()));
    let app = test_app(&pool, verifier.clone());
    let _uid = create_user(&app).await;
    let response = app.oneshot(Request::builder().method(http::Method::GET).uri("/api/challenge").header("Authorization", bearer("t2")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    assert!(body["challenge_id"].is_string());
    assert!(body["problem"].is_string());
    assert!(body["difficulty"].is_number());
    assert!(body["reward"].is_number());
    assert!(body["expires_at"].is_string());
    assert!(body.get("solution").is_none(), "solution leaked");
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM challenges WHERE status = 'PENDING'").fetch_one(&pool).await.unwrap();
    assert_eq!(count.0, 1);
}

#[tokio::test]
async fn challenge_unauth_returns_401() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::rejecting());
    let app = test_app(&pool, verifier);
    let response = app.oneshot(Request::builder().method(http::Method::GET).uri("/api/challenge").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), 401);
}

// ---- POST /api/mint ----

#[tokio::test]
async fn mint_correct_answer_credits_earnings() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-mint-001".into(), "mint@example.com".into()));
    let app = test_app(&pool, verifier.clone());
    let uid = create_user(&app).await;
    let cid = create_test_challenge(&pool, uid).await;
    let body_str = format!(r#"{{"challenge_id": "{}", "answer": 42}}"#, cid);
    let response = app.oneshot(Request::builder().method(http::Method::POST).uri("/api/mint").header("Authorization", bearer("t2")).header("Content-Type", "application/json").body(Body::from(body_str)).unwrap()).await.unwrap();
    let resp_bytes = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&resp_bytes).unwrap();
    assert_eq!(body["status"], "CLAIMED");
    assert_eq!(body["reward"], 20);
    let status: (String,) = sqlx::query_as("SELECT status FROM challenges WHERE id = $1").bind(cid).fetch_one(&pool).await.unwrap();
    assert_eq!(status.0, "CLAIMED");
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM earnings WHERE challenge_id = $1").bind(cid).fetch_one(&pool).await.unwrap();
    assert_eq!(count.0, 1);
}

#[tokio::test]
async fn mint_wrong_answer_returns_422_expires_challenge() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-mint-002".into(), "mint2@example.com".into()));
    let app = test_app(&pool, verifier.clone());
    let uid = create_user(&app).await;
    let cid = create_test_challenge(&pool, uid).await;
    let response = app.oneshot(Request::builder().method(http::Method::POST).uri("/api/mint").header("Authorization", bearer("t2")).header("Content-Type", "application/json").body(Body::from(serde_json::json!({"challenge_id": cid, "answer": 999}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), 422);
    let status: (String,) = sqlx::query_as("SELECT status FROM challenges WHERE id = $1").bind(cid).fetch_one(&pool).await.unwrap();
    assert_eq!(status.0, "EXPIRED");
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM earnings WHERE challenge_id = $1").bind(cid).fetch_one(&pool).await.unwrap();
    assert_eq!(count.0, 0);
}

#[tokio::test]
async fn mint_replay_already_claimed_returns_409() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-mint-003".into(), "mint3@example.com".into()));
    let app = test_app(&pool, verifier.clone());
    let uid = create_user(&app).await;
    let cid = create_test_challenge(&pool, uid).await;
    let r1 = app.clone().oneshot(Request::builder().method(http::Method::POST).uri("/api/mint").header("Authorization", bearer("t2")).header("Content-Type", "application/json").body(Body::from(serde_json::json!({"challenge_id": cid, "answer": 42}).to_string())).unwrap()).await.unwrap();
    assert_eq!(r1.status(), 200);
    let r2 = app.oneshot(Request::builder().method(http::Method::POST).uri("/api/mint").header("Authorization", bearer("t3")).header("Content-Type", "application/json").body(Body::from(serde_json::json!({"challenge_id": cid, "answer": 42}).to_string())).unwrap()).await.unwrap();
    assert_eq!(r2.status(), 409);
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM earnings WHERE challenge_id = $1").bind(cid).fetch_one(&pool).await.unwrap();
    assert_eq!(count.0, 1);
}

#[tokio::test]
async fn mint_expired_challenge_returns_410() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-mint-004".into(), "mint4@example.com".into()));
    let app = test_app(&pool, verifier.clone());
    let uid = create_user(&app).await;
    let cid = create_expired_challenge(&pool, uid).await;
    let response = app.oneshot(Request::builder().method(http::Method::POST).uri("/api/mint").header("Authorization", bearer("t2")).header("Content-Type", "application/json").body(Body::from(serde_json::json!({"challenge_id": cid, "answer": 2}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), 410);
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM earnings WHERE challenge_id = $1").bind(cid).fetch_one(&pool).await.unwrap();
    assert_eq!(count.0, 0);
}

#[tokio::test]
async fn mint_identity_from_jwt_not_body() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-mint-005".into(), "real@example.com".into()));
    let app = test_app(&pool, verifier.clone());
    let uid = create_user(&app).await;
    let cid = create_test_challenge(&pool, uid).await;
    let response = app.oneshot(Request::builder().method(http::Method::POST).uri("/api/mint").header("Authorization", bearer("t2")).header("Content-Type", "application/json").body(Body::from(serde_json::json!({"challenge_id": cid, "answer": 42, "user_id": 999}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), 200);
    let row: (i64,) = sqlx::query_as("SELECT user_id FROM earnings WHERE challenge_id = $1").bind(cid).fetch_one(&pool).await.unwrap();
    assert_eq!(row.0, uid, "should credit JWT user, not forged body user_id");
}
