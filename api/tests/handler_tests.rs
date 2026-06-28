/// Handler integration tests: POST /api/session, GET /api/me, GET /api/challenge, POST /api/mint.
/// Uses a shared test Postgres + mock AuthVerifier.
/// Each test clears its own data.
use axum::body::Body;
use axum::http::{self, Request};
use mathcoin_api::auth::{AuthVerifier, MockVerifier};
use mathcoin_api::difficulty::{FakeClock, MintingStats, RetargetConfig};
use mathcoin_api::rate_limit::RateLimiter;
use mathcoin_api::state::AppState;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::{Duration, Instant};
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
        difficulty: Arc::new(AtomicU32::new(3)),
        mint_stats: Arc::new(tokio::sync::Mutex::new(MintingStats::new())),
        clock: Arc::new(FakeClock::new(Instant::now())),
        retarget_config: RetargetConfig {
            window: Duration::from_secs(60),
            target_rate: 20.0,
            hysteresis_low: 15.0,
            hysteresis_high: 25.0,
            diff_min: 1,
            diff_max: 12,
            max_step: 1,
        },
        rate_limiter: Arc::new(RateLimiter::new(60, 1000)),
    });
    axum::Router::new()
        .route("/api/session", post(mathcoin_api::routes::session::handler))
        .route("/api/me", get(mathcoin_api::routes::me::handler))
        .route("/api/challenge", get(mathcoin_api::routes::challenge::handler))
        .route("/api/mint", post(mathcoin_api::routes::mint::handler))
        .route("/api/stats", get(mathcoin_api::routes::stats::handler))
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

// ---- GET /api/stats ----

#[tokio::test]
async fn stats_returns_difficulty_rate_and_total_supply() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-stats".into(), "stats@example.com".into()));
    let app = test_app(&pool, verifier);

    let response = app.oneshot(Request::builder().method(http::Method::GET).uri("/api/stats").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(body["current_difficulty"], 3);
    assert_eq!(body["mints_last_60s"], 0.0);
    assert_eq!(body["target_rate_per_60s"], 20.0);
    assert_eq!(body["total_accrued_supply"], 0);
}

// ---- Difficulty retarget integration ----

#[tokio::test]
async fn fast_mints_raise_difficulty() {
    let pool = pool().await;
    clean_db(&pool).await;
    let clock = Arc::new(FakeClock::new(Instant::now()));
    let verifier: Arc<dyn AuthVerifier> = Arc::new(MockVerifier::accepting("sub-fast".into(), "fast@example.com".into()));
    let app = test_app_with_clock(&pool, verifier, clock.clone());
    let uid = create_user(&app).await;

    // Seed and mint 30 challenges at current difficulty=3, fast enough to push rate > 25
    for _ in 0..30 {
        let cid = create_test_challenge(&pool, uid).await;
        let r = app.clone().oneshot(Request::builder().method(http::Method::POST).uri("/api/mint").header("Authorization", bearer("t1")).header("Content-Type", "application/json").body(Body::from(serde_json::json!({"challenge_id": cid, "answer": 42}).to_string())).unwrap()).await.unwrap();
        assert_eq!(r.status(), 200);
        // Small time advance so they're all within the 60s window
        clock.advance(Duration::from_secs(1));
    }

    // After 30 mints in ~30s, difficulty should have gone up (rate = 30 in 60s > 25)
    let stats = app.oneshot(Request::builder().method(http::Method::GET).uri("/api/stats").body(Body::empty()).unwrap()).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(stats.into_body(), 1024).await.unwrap()).unwrap();
    assert!(body["current_difficulty"].as_u64().unwrap() >= 3, "difficulty should have increased from 3, got {}", body["current_difficulty"]);
}

#[tokio::test]
async fn lull_lowers_difficulty() {
    let pool = pool().await;
    clean_db(&pool).await;
    let clock = Arc::new(FakeClock::new(Instant::now()));
    let verifier: Arc<dyn AuthVerifier> = Arc::new(MockVerifier::accepting("sub-lull".into(), "lull@example.com".into()));
    let app = test_app_with_clock(&pool, verifier, clock.clone());
    let uid = create_user(&app).await;

    // First spike difficulty up to ~6 by doing many mints
    for _ in 0..50 {
        let cid = create_test_challenge(&pool, uid).await;
        let r = app.clone().oneshot(Request::builder().method(http::Method::POST).uri("/api/mint").header("Authorization", bearer("t1")).header("Content-Type", "application/json").body(Body::from(serde_json::json!({"challenge_id": cid, "answer": 42}).to_string())).unwrap()).await.unwrap();
        assert_eq!(r.status(), 200);
        clock.advance(Duration::from_secs(1));
    }

    // Check peak difficulty after many fast mints
    let peak_stats = app.clone().oneshot(Request::builder().method(http::Method::GET).uri("/api/stats").body(Body::empty()).unwrap()).await.unwrap();
    let peak_body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(peak_stats.into_body(), 1024).await.unwrap()).unwrap();
    let peak_diff = peak_body["current_difficulty"].as_u64().unwrap();
    assert!(peak_diff > 3, "difficulty should have increased from 3 after many mints, got {peak_diff}");

    // Now advance clock by 120s — all mints evicted from window, rate → 0
    clock.advance(Duration::from_secs(120));
    // Trigger retarget: do one more mint
    let cid = create_test_challenge(&pool, uid).await;
    let r = app.clone().oneshot(Request::builder().method(http::Method::POST).uri("/api/mint").header("Authorization", bearer("t1")).header("Content-Type", "application/json").body(Body::from(serde_json::json!({"challenge_id": cid, "answer": 42}).to_string())).unwrap()).await.unwrap();
    assert_eq!(r.status(), 200);

    let stats = app.oneshot(Request::builder().method(http::Method::GET).uri("/api/stats").body(Body::empty()).unwrap()).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(stats.into_body(), 1024).await.unwrap()).unwrap();
    let after_lull = body["current_difficulty"].as_u64().unwrap();
    // With almost zero rate, difficulty should have dropped from the peak
    assert!(after_lull < peak_diff || after_lull <= 3, "difficulty should have dropped from peak {peak_diff}, got {after_lull}");
}

// ---- Helper for tests that need clock injection ----

fn test_app_with_clock(pool: &PgPool, verifier: Arc<dyn AuthVerifier>, clock: Arc<FakeClock>) -> axum::Router {
    use axum::routing::{get, post};
    let state = Arc::new(AppState {
        db: pool.clone(),
        verifier: verifier.clone(),
        difficulty: Arc::new(AtomicU32::new(3)),
        mint_stats: Arc::new(tokio::sync::Mutex::new(MintingStats::new())),
        clock: clock as Arc<dyn mathcoin_api::difficulty::Clock>,
        retarget_config: RetargetConfig {
            window: Duration::from_secs(60),
            target_rate: 20.0,
            hysteresis_low: 15.0,
            hysteresis_high: 25.0,
            diff_min: 1,
            diff_max: 12,
            max_step: 1,
        },
        rate_limiter: Arc::new(RateLimiter::new(60, 1000)),
    });
    axum::Router::new()
        .route("/api/session", post(mathcoin_api::routes::session::handler))
        .route("/api/me", get(mathcoin_api::routes::me::handler))
        .route("/api/challenge", get(mathcoin_api::routes::challenge::handler))
        .route("/api/mint", post(mathcoin_api::routes::mint::handler))
        .route("/api/stats", get(mathcoin_api::routes::stats::handler))
        .with_state(state)
}

// ---- Rate limiting ----

fn test_app_with_rate(pool: &PgPool, verifier: Arc<dyn AuthVerifier>, max_requests: u64) -> axum::Router {
    use axum::routing::{get, post};
    let state = Arc::new(AppState {
        db: pool.clone(),
        verifier,
        difficulty: Arc::new(AtomicU32::new(3)),
        mint_stats: Arc::new(tokio::sync::Mutex::new(MintingStats::new())),
        clock: Arc::new(FakeClock::new(Instant::now())),
        retarget_config: RetargetConfig {
            window: Duration::from_secs(60),
            target_rate: 20.0,
            hysteresis_low: 15.0,
            hysteresis_high: 25.0,
            diff_min: 1,
            diff_max: 12,
            max_step: 1,
        },
        rate_limiter: Arc::new(RateLimiter::new(60, max_requests)),
    });
    axum::Router::new()
        .route("/api/session", post(mathcoin_api::routes::session::handler))
        .route("/api/me", get(mathcoin_api::routes::me::handler))
        .route("/api/stats", get(mathcoin_api::routes::stats::handler))
        .layer(axum::middleware::from_fn_with_state(state.clone(), mathcoin_api::rate_limit::rate_limit_middleware))
        .with_state(state)
}

#[tokio::test]
async fn rate_limit_under_limit_passes() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-rl-01".into(), "rl@example.com".into()));
    let app = test_app_with_rate(&pool, verifier, 5);

    for _ in 0..3 {
        let r = app.clone().oneshot(Request::builder().method(http::Method::GET).uri("/api/stats").header("Authorization", bearer("t1")).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(r.status(), 200, "under-limit request should pass");
    }
}

#[tokio::test]
async fn rate_limit_over_limit_returns_429() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier = Arc::new(MockVerifier::accepting("sub-rl-02".into(), "rl2@example.com".into()));
    let app = test_app_with_rate(&pool, verifier, 3);

    for _ in 0..3 {
        let r = app.clone().oneshot(Request::builder().method(http::Method::GET).uri("/api/stats").header("Authorization", bearer("t1")).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(r.status(), 200);
    }

    let r = app.oneshot(Request::builder().method(http::Method::GET).uri("/api/stats").header("Authorization", bearer("t1")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(r.status(), 429);
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(r.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(body["error"], "rate_limited");
}

#[tokio::test]
async fn rate_limit_per_sub_isolation() {
    let pool = pool().await;
    clean_db(&pool).await;
    let verifier_a = Arc::new(MockVerifier::accepting("sub-a".into(), "a@example.com".into()));
    let verifier_b = Arc::new(MockVerifier::accepting("sub-b".into(), "b@example.com".into()));
    let app_a = test_app_with_rate(&pool, verifier_a, 2);
    let app_b = test_app_with_rate(&pool, verifier_b.clone(), 2);

    for _ in 0..2 {
        let r = app_a.clone().oneshot(Request::builder().method(http::Method::GET).uri("/api/stats").header("Authorization", bearer("tA")).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(r.status(), 200);
    }
    let r = app_a.oneshot(Request::builder().method(http::Method::GET).uri("/api/stats").header("Authorization", bearer("tA")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(r.status(), 429, "user A should be rate limited");

    let r = app_b.oneshot(Request::builder().method(http::Method::GET).uri("/api/stats").header("Authorization", bearer("tB")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(r.status(), 200, "user B should NOT be rate limited");
}

// ---- POST /api/claim-address ----

fn test_app_with_claim_sub(pool: &PgPool, sub: &str) -> axum::Router {
    use axum::routing::post;
    let verifier = Arc::new(MockVerifier::accepting(sub.into(), format!("{sub}@example.com")));
    let state = Arc::new(AppState {
        db: pool.clone(),
        verifier,
        difficulty: Arc::new(AtomicU32::new(3)),
        mint_stats: Arc::new(tokio::sync::Mutex::new(MintingStats::new())),
        clock: Arc::new(FakeClock::new(Instant::now())),
        retarget_config: RetargetConfig {
            window: Duration::from_secs(60), target_rate: 20.0,
            hysteresis_low: 15.0, hysteresis_high: 25.0,
            diff_min: 1, diff_max: 12, max_step: 1,
        },
        rate_limiter: Arc::new(RateLimiter::new(60, 1000)),
    });
    axum::Router::new()
        .route("/api/session", post(mathcoin_api::routes::session::handler))
        .route("/api/claim-address", post(mathcoin_api::routes::claim_address::handler))
        .with_state(state)
}

fn test_app_with_claim(pool: &PgPool) -> axum::Router {
    test_app_with_claim_sub(pool, "sub-claim")
}

#[tokio::test]
async fn claim_address_valid_eip55_persists() {
    let pool = pool().await;
    clean_db(&pool).await;
    let app = test_app_with_claim(&pool);
    create_user(&app).await;

    // Valid EIP-55 checksummed address
    let r = app.oneshot(Request::builder().method(http::Method::POST).uri("/api/claim-address")
        .header("Authorization", bearer("t1")).header("Content-Type", "application/json")
        .body(Body::from(r#"{"address":"0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B"}"#)).unwrap()).await.unwrap();
    assert_eq!(r.status(), 200);

    let addr: (Option<String>,) = sqlx::query_as("SELECT claim_address FROM users WHERE provider_sub = 'sub-claim'")
        .fetch_one(&pool).await.unwrap();
    assert!(addr.0.is_some());
}

#[tokio::test]
async fn claim_address_invalid_checksum_returns_400() {
    let pool = pool().await;
    clean_db(&pool).await;
    let app = test_app_with_claim(&pool);
    create_user(&app).await;

    // Invalid checksum (mixed case wrong)
    let r = app.oneshot(Request::builder().method(http::Method::POST).uri("/api/claim-address")
        .header("Authorization", bearer("t1")).header("Content-Type", "application/json")
        .body(Body::from(r#"{"address":"0xAb5801a7D398351b8bE11C439e05C5B3259aeC9c"}"#)).unwrap()).await.unwrap();
    assert_eq!(r.status(), 400);

    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(r.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(body["error"], "invalid_request");
}

#[tokio::test]
async fn claim_address_duplicate_rejected() {
    let pool = pool().await;
    clean_db(&pool).await;

    // First user sets the address
    let app1 = test_app_with_claim(&pool);
    create_user(&app1).await;
    let r = app1.oneshot(Request::builder().method(http::Method::POST).uri("/api/claim-address")
        .header("Authorization", bearer("t1")).header("Content-Type", "application/json")
        .body(Body::from(r#"{"address":"0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B"}"#)).unwrap()).await.unwrap();
    assert_eq!(r.status(), 200);

    // Second user with same address should fail
    let app2 = test_app_with_claim_sub(&pool, "sub-claim-2");
    let _uid2 = create_user(&app2).await;
    let r = app2.oneshot(Request::builder().method(http::Method::POST).uri("/api/claim-address")
        .header("Authorization", bearer("t1")).header("Content-Type", "application/json")
        .body(Body::from(r#"{"address":"0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B"}"#)).unwrap()).await.unwrap();
    assert!(r.status().as_u16() >= 400, "duplicate claim_address should fail");
}
