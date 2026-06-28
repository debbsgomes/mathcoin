/// Concurrency test harness for MathCoin mint endpoint.
/// Proves the harness genuinely parallelizes before testing hot contention.
use axum::body::Body;
use axum::http::{self, Request};
use mathcoin_api::auth::MockVerifier;
use mathcoin_api::state::AppState;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Barrier;
use tower::ServiceExt;
use uuid::Uuid;

const POOL_SIZE: u32 = 40; // Must be >= concurrency level N

async fn test_pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://mathcoin:mathcoin@localhost:5432/mathcoin_test".into());
    PgPoolOptions::new()
        .max_connections(POOL_SIZE)
        .connect(&url)
        .await
        .unwrap()
}

fn test_app(pool: &PgPool) -> axum::Router {
    use axum::routing::{get, post};
    let verifier = Arc::new(MockVerifier::accepting(
        "concurrent-user".into(),
        "concurrent@example.com".into(),
    ));
    let state = Arc::new(AppState {
        db: pool.clone(),
        verifier,
    });
    axum::Router::new()
        .route("/api/session", post(mathcoin_api::routes::session::handler))
        .route("/api/mint", post(mathcoin_api::routes::mint::handler))
        .with_state(state)
}

fn bearer() -> String {
    "Bearer valid-token".to_string()
}

/// Seed a user and return the user_id.
async fn seed_user(pool: &PgPool, sub: &str, email: &str) -> i64 {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO users (provider_sub, email) VALUES ($1, $2)
         ON CONFLICT (provider_sub) DO UPDATE SET email = EXCLUDED.email
         RETURNING id",
    )
    .bind(sub)
    .bind(email)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

/// Seed a PENDING challenge and return the challenge_id.
/// The challenge is "40 + 2" with solution 42, reward 20.
async fn seed_challenge(pool: &PgPool, user_id: i64) -> Uuid {
    let cid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO challenges (id, user_id, problem, solution, difficulty, reward, status, expires_at)
         VALUES ($1, $2, '40 + 2', 42, 3, 20, 'PENDING', now() + INTERVAL '10 minutes')",
    )
    .bind(cid)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
    cid
}

#[derive(Debug)]
struct MintResult {
    status: u16,
    body: serde_json::Value,
}

/// Fire N concurrent POST /api/mint requests as simultaneously as possible.
/// Each entry is (challenge_id, answer).
async fn concurrent_mint(
    pool: &PgPool,
    entries: Vec<(Uuid, i64)>,
) -> Vec<MintResult> {
    let n = entries.len();
    let barrier = Arc::new(Barrier::new(n));
    let mut handles = Vec::with_capacity(n);

    for (cid, answer) in entries {
        let pool = pool.clone();
        let barrier = barrier.clone();
        let handle = tokio::spawn(async move {
            // Wait for all tasks to be ready
            barrier.wait().await;
            // Each task creates its own app (with its own pool clone, but shared underlying pool)
            let app = test_app(&pool);
            let body = serde_json::json!({"challenge_id": cid, "answer": answer});
            let resp = app
                .oneshot(
                    Request::builder()
                        .method(http::Method::POST)
                        .uri("/api/mint")
                        .header("Authorization", bearer())
                        .header("Content-Type", "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = resp.status().as_u16();
            let resp_body: serde_json::Value =
                serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1024).await.unwrap())
                    .unwrap();
            MintResult {
                status,
                body: resp_body,
            }
        });
        handles.push(handle);
    }

    let mut results = Vec::with_capacity(n);
    for handle in handles {
        results.push(handle.await.unwrap());
    }
    results
}

// ---- Smoke test: N distinct challenges, all should succeed ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_distinct_challenges_all_succeed() {
    let pool = test_pool().await;

    // Clean up from previous runs
    sqlx::query("DELETE FROM earnings").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM challenges").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM users").execute(&pool).await.unwrap();

    let n = 30;

    // Seed one user that matches the mock verifier's sub="concurrent-user"
    let uid = seed_user(&pool, "concurrent-user", "concurrent@example.com").await;

    // Seed N distinct challenges for that user — all committed before fan-out
    let mut entries = Vec::with_capacity(n);
    for _ in 0..n {
        let cid = seed_challenge(&pool, uid).await;
        entries.push((cid, 42i64));
    }

    // Fire N concurrent mints
    let results = concurrent_mint(&pool, entries).await;

    // Assert: all N return 200
    let successes: Vec<_> = results.iter().filter(|r| r.status == 200).collect();
    assert_eq!(
        successes.len(),
        n,
        "expected all {} to succeed, got {} successes. Failures: {:?}",
        n,
        successes.len(),
        results.iter().filter(|r| r.status != 200).collect::<Vec<_>>()
    );

    // Each result should show CLAIMED status and reward 20
    for r in &results {
        assert_eq!(r.body["status"], "CLAIMED");
        assert_eq!(r.body["reward"], 20);
        assert!(r.body["balance"].as_i64().unwrap() > 0);
    }

    // Assert: N earnings rows total (one per challenge)
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM earnings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0 as usize, n, "should have exactly {n} earnings rows");

    // Assert: all challenges are CLAIMED
    let claimed: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM challenges WHERE status = 'CLAIMED'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(claimed.0 as usize, n);

    // Assert: user's balance equals sum of all rewards
    let balance: (Option<i64>,) = sqlx::query_as(
        "SELECT SUM(amount)::BIGINT FROM earnings WHERE user_id = $1",
    )
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(balance.0, Some((n * 20) as i64), "user should have balance {}", n * 20);
}
