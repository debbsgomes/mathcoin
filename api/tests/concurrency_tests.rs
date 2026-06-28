/// Concurrency test harness for MathCoin mint endpoint.
/// Proves the harness genuinely parallelizes before testing hot contention.
///
/// NOTE: Concurrency tests share a single test database. Run with `--test-threads=1`
/// to avoid cross-test contamination:
///   cargo test --test concurrency_tests -- --test-threads=1
use axum::body::Body;
use axum::http::{self, Request};
use mathcoin_api::auth::MockVerifier;
use mathcoin_api::difficulty::{FakeClock, MintingStats, RetargetConfig};
use mathcoin_api::rate_limit::RateLimiter;
use mathcoin_api::state::AppState;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Barrier;
use tower::ServiceExt;
use uuid::Uuid;

const POOL_SIZE: u32 = 220; // Each request holds up to 2 connections (SELECT + transaction)

async fn test_pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://mathcoin:mathcoin@localhost:5432/mathcoin_test".into());
    PgPoolOptions::new()
        .max_connections(POOL_SIZE)
        .connect(&url)
        .await
        .unwrap()
}

fn test_app(pool: &PgPool, sub: &str) -> axum::Router {
    use axum::routing::{get, post};
    let verifier = Arc::new(MockVerifier::accepting(
        sub.into(),
        "concurrent@example.com".into(),
    ));
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
        rate_limiter: Arc::new(RateLimiter::new(60, 10000)),
        onchain_config: None,
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
/// Each entry is (challenge_id, answer). Uses the given sub for auth.
async fn concurrent_mint(
    pool: &PgPool,
    sub: &str,
    entries: Vec<(Uuid, i64)>,
) -> Vec<MintResult> {
    let n = entries.len();
    let barrier = Arc::new(Barrier::new(n));
    let mut handles = Vec::with_capacity(n);

    for (cid, answer) in entries {
        let pool = pool.clone();
        let barrier = barrier.clone();
        let sub = sub.to_string();
        let handle = tokio::spawn(async move {
            barrier.wait().await;
            let app = test_app(&pool, &sub);
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

    // Seed one user that matches the mock verifier's sub
    let test_sub = "conc-distinct";
    let uid = seed_user(&pool, test_sub, "conc-distinct@example.com").await;

    // Seed N distinct challenges for that user — all committed before fan-out
    let mut entries = Vec::with_capacity(n);
    for _ in 0..n {
        let cid = seed_challenge(&pool, uid).await;
        entries.push((cid, 42i64));
    }

    // Fire N concurrent mints
    let results = concurrent_mint(&pool, test_sub, entries).await;

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

// ---- Hot-contention test: 100 racers on ONE challenge ----

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn hundred_racers_single_challenge_exactly_one_credit() {
    let pool = test_pool().await;

    // Clean up
    sqlx::query("DELETE FROM earnings").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM challenges").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM users").execute(&pool).await.unwrap();

    let test_sub = "conc-race";
    let uid = seed_user(&pool, test_sub, "conc-race@example.com").await;

    // Seed EXACTLY ONE PENDING challenge
    let cid = seed_challenge(&pool, uid).await;

    let concurrency = 100;

    // Build 100 entries — all for the SAME challenge_id with the correct answer
    let entries: Vec<_> = (0..concurrency)
        .map(|_| (cid, 42i64))
        .collect();

    // Fire 100 concurrent mints against the same challenge
    let results = concurrent_mint(&pool, test_sub, entries).await;

    // ---- INVARIANT 1: exactly ONE 200 ----
    let successes: Vec<_> = results.iter().filter(|r| r.status == 200).collect();
    assert_eq!(
        successes.len(),
        1,
        "expected exactly 1 success (200), got {}. All statuses: {:?}",
        successes.len(),
        results.iter().map(|r| r.status).collect::<Vec<_>>()
    );

    // ---- INVARIANT 2: other 99 are 409 ----
    let conflicts: Vec<_> = results.iter().filter(|r| r.status == 409).collect();
    assert_eq!(
        conflicts.len(),
        concurrency - 1,
        "expected {} conflicts (409), got {}. All statuses: {:?}",
        concurrency - 1,
        conflicts.len(),
        results
            .iter()
            .filter(|r| r.status != 200 && r.status != 409)
            .map(|r| (r.status, &r.body))
            .collect::<Vec<_>>()
    );

    // ---- INVARIANT 3: exactly ONE earnings row ----
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM earnings WHERE challenge_id = $1",
    )
    .bind(cid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        count.0, 1,
        "should have exactly 1 earnings row, got {}",
        count.0
    );

    // ---- INVARIANT 4: challenge is CLAIMED ----
    let status: (String,) =
        sqlx::query_as("SELECT status FROM challenges WHERE id = $1")
            .bind(cid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status.0, "CLAIMED", "challenge should be CLAIMED");

    // ---- INVARIANT 5: balance increased by exactly ONE reward ----
    let balance: (Option<i64>,) = sqlx::query_as(
        "SELECT SUM(amount)::BIGINT FROM earnings WHERE user_id = $1",
    )
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        balance.0,
        Some(20),
        "balance should be exactly one reward (20), got {:?}",
        balance.0
    );
}

// ---- Correct-vs-wrong race: mixed answers on the SAME challenge ----

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_correct_vs_wrong_at_most_one_credit() {
    let pool = test_pool().await;

    sqlx::query("DELETE FROM earnings").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM challenges").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM users").execute(&pool).await.unwrap();

    let test_sub = "conc-cvsw";
    let uid = seed_user(&pool, test_sub, "conc-cvsw@example.com").await;
    let cid = seed_challenge(&pool, uid).await;

    let concurrency = 50;

    // Half correct (42), half wrong (999)
    let mut entries = Vec::with_capacity(concurrency);
    for i in 0..concurrency {
        let answer = if i % 2 == 0 { 42i64 } else { 999i64 };
        entries.push((cid, answer));
    }

    let results = concurrent_mint(&pool, test_sub, entries).await;

    // INVARIANT: at most ONE earnings row (0 or 1, never >1)
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM earnings WHERE challenge_id = $1",
    )
    .bind(cid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        count.0 <= 1,
        "at most 1 earnings row expected, got {}",
        count.0
    );

    // INVARIANT: exactly ONE terminal state — either CLAIMED or EXPIRED, never PENDING
    let status: (String,) =
        sqlx::query_as("SELECT status FROM challenges WHERE id = $1")
            .bind(cid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        status.0 == "CLAIMED" || status.0 == "EXPIRED",
        "challenge must be in a terminal state (CLAIMED or EXPIRED), got {}",
        status.0
    );

    // INVARIANT: zero or one credit, consistent with status
    if status.0 == "CLAIMED" {
        assert_eq!(count.0, 1, "CLAIMED must have exactly 1 credit");
        let winners: Vec<_> = results.iter().filter(|r| r.status == 200).collect();
        assert_eq!(winners.len(), 1, "CLAIMED requires exactly 1 winner (200)");
    } else {
        assert_eq!(count.0, 0, "EXPIRED must have 0 credits");
        // Wrong-answer winners get 422, losers get 409
        let non_409: Vec<_> = results.iter().filter(|r| r.status == 422).collect();
        assert!(non_409.len() <= 1, "at most one 422 (the wrong-answer winner)");
    }

    // Ensure no unexpected statuses
    for r in &results {
        assert!(
            r.status == 200 || r.status == 409 || r.status == 422,
            "unexpected status {}: {:?}",
            r.status,
            r.body
        );
    }

    // Balance must be 0 or 20, never anything else
    let balance: (Option<i64>,) = sqlx::query_as(
        "SELECT SUM(amount)::BIGINT FROM earnings WHERE user_id = $1",
    )
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();
    let bal = balance.0.unwrap_or(0);
    assert!(
        bal == 0 || bal == 20,
        "balance must be 0 or 20, got {bal}",
    );
}

// ---- Replay race: already-CLAIMED challenge, concurrent mints → all 409 ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_replay_already_claimed_all_409() {
    let pool = test_pool().await;

    sqlx::query("DELETE FROM earnings").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM challenges").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM users").execute(&pool).await.unwrap();

    let test_sub = "conc-replay";
    let uid = seed_user(&pool, test_sub, "conc-replay@example.com").await;
    let cid = seed_challenge(&pool, uid).await;

    // Pre-claim the challenge (single-threaded)
    let entries: Vec<_> = vec![(cid, 42i64)];
    let results = concurrent_mint(&pool, test_sub, entries).await;
    assert_eq!(results[0].status, 200, "pre-claim should succeed");
    assert_eq!(results[0].body["status"], "CLAIMED");

    let pre_balance: (Option<i64>,) = sqlx::query_as(
        "SELECT SUM(amount)::BIGINT FROM earnings WHERE user_id = $1",
    )
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Now fire 50 concurrent replays
    let concurrency = 50;
    let entries: Vec<_> = (0..concurrency).map(|_| (cid, 42i64)).collect();
    let results = concurrent_mint(&pool, test_sub, entries).await;

    // ALL must be 409
    for r in &results {
        assert_eq!(
            r.status, 409,
            "replay must return 409, got {}: {:?}",
            r.status, r.body
        );
    }

    // NO additional earnings rows
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM earnings WHERE challenge_id = $1",
    )
    .bind(cid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count.0, 1, "should still have exactly 1 earnings row");

    // Balance unchanged
    let post_balance: (Option<i64>,) = sqlx::query_as(
        "SELECT SUM(amount)::BIGINT FROM earnings WHERE user_id = $1",
    )
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        post_balance.0, pre_balance.0,
        "balance must be unchanged after replays"
    );
}

// ---- Expired challenge: concurrent mints → all 410/409, no credit ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_mint_vs_expired_no_credit() {
    let pool = test_pool().await;

    sqlx::query("DELETE FROM earnings").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM challenges").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM users").execute(&pool).await.unwrap();

    let test_sub = "conc-expired";
    let uid = seed_user(&pool, test_sub, "conc-expired@example.com").await;

    // Seed an already-expired challenge
    let cid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO challenges (id, user_id, problem, solution, difficulty, reward, status, expires_at)
         VALUES ($1, $2, '1 + 1', 2, 1, 5, 'PENDING', now() - INTERVAL '1 minute')",
    )
    .bind(cid)
    .bind(uid)
    .execute(&pool)
    .await
    .unwrap();

    let concurrency = 30;
    let entries: Vec<_> = (0..concurrency).map(|_| (cid, 2i64)).collect();
    let results = concurrent_mint(&pool, test_sub, entries).await;

    // All must be 410 or 409, never 200
    for r in &results {
        assert!(
            r.status == 410 || r.status == 409,
            "expired must return 410 or 409, got {}: {:?}",
            r.status,
            r.body
        );
    }

    // At least one 410 (the first one marks it EXPIRED)
    let expired_count = results.iter().filter(|r| r.status == 410).count();
    assert!(expired_count >= 1, "at least one request should get 410");

    // ZERO earnings rows
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM earnings WHERE challenge_id = $1",
    )
    .bind(cid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count.0, 0, "expired challenge must have 0 earnings rows");

    // Balance unchanged
    let balance: (Option<i64>,) = sqlx::query_as(
        "SELECT SUM(amount)::BIGINT FROM earnings WHERE user_id = $1",
    )
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(balance.0.unwrap_or(0), 0, "balance must be 0 for expired challenge");
}

// ---- Property test: balance = sum of correctly-credited rewards ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn property_no_double_credit_multiple_users() {
    let pool = test_pool().await;

    sqlx::query("DELETE FROM earnings").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM challenges").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM users").execute(&pool).await.unwrap();

    let num_users = 10;
    let challenges_per_user = 8;
    let correct_batch = 5;
    let wrong_batch = 3;

    // Seed users
    let mut user_ids = Vec::new();
    for i in 0..num_users {
        let sub = format!("prop-user-{i}");
        let uid = seed_user(&pool, &sub, &format!("prop{i}@test.com")).await;
        user_ids.push((uid, sub));
    }

    let mut challenge_map: std::collections::HashMap<Uuid, i64> = std::collections::HashMap::new();

    // Fire per-user batches concurrently
    for (uid, sub) in &user_ids {
        let mut batch = Vec::new();
        for _ in 0..challenges_per_user {
            let cid = seed_challenge(&pool, *uid).await;
            challenge_map.insert(cid, *uid);
            for _ in 0..correct_batch {
                batch.push((cid, 42i64));
            }
            for _ in 0..wrong_batch {
                batch.push((cid, 999i64));
            }
        }
        concurrent_mint(&pool, sub, batch).await;
    }

    // INVARIANT 1: per-challenge, at most 1 credit
    for (cid, uid) in &challenge_map {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM earnings WHERE challenge_id = $1",
        )
        .bind(cid)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(count.0 <= 1, "challenge {cid} has {n} earnings rows", n = count.0);

        let status: (String,) =
            sqlx::query_as("SELECT status FROM challenges WHERE id = $1")
                .bind(cid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            status.0 == "CLAIMED" || status.0 == "EXPIRED",
            "challenge {cid} in non-terminal state {s}", s = status.0
        );
    }

    // INVARIANT 2: each user's balance = SUM of correctly-credited rewards
    for (uid, _sub) in &user_ids {
        let balance: (Option<i64>,) = sqlx::query_as(
            "SELECT SUM(amount)::BIGINT FROM earnings WHERE user_id = $1",
        )
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();
        let actual = balance.0.unwrap_or(0);

        // Expected: one reward per challenge that was CLAIMED
        let expected: (Option<i64>,) = sqlx::query_as(
            "SELECT COALESCE(SUM(reward), 0)::BIGINT FROM challenges
             WHERE user_id = $1 AND status = 'CLAIMED'",
        )
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(
            actual,
            expected.0.unwrap_or(0),
            "user {uid} balance mismatch: earnings={actual}, expected from CLAIMED challenges={}",
            expected.0.unwrap_or(0)
        );

        // INVARIANT 3: balance never exceeds sum of all awarded rewards
        let total_possible: (Option<i64>,) = sqlx::query_as(
            "SELECT COALESCE(SUM(reward), 0)::BIGINT FROM challenges WHERE user_id = $1",
        )
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(
            actual <= total_possible.0.unwrap_or(0),
            "balance {actual} exceeds total possible {total}",
            total = total_possible.0.unwrap_or(0)
        );
    }
}

// ---- Broad concurrent load: many users + challenges at once ----

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn broad_concurrent_load_no_cross_contamination() {
    let pool = test_pool().await;

    sqlx::query("DELETE FROM earnings").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM challenges").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM users").execute(&pool).await.unwrap();

    let num_users = 8;
    let challenges_per_user = 10;
    let racers_per_challenge = 5; // modest concurrency per challenge

    let mut user_data = Vec::new();
    for i in 0..num_users {
        let sub = format!("broad-user-{i}");
        let uid = seed_user(&pool, &sub, &format!("broad{i}@test.com")).await;
        user_data.push((uid, sub));
    }

    // Build all entries: for each user, challenges with mixed correct/wrong answers
    let mut all_handles = Vec::new();

    for (uid, sub) in &user_data {
        let mut entries = Vec::new();
        let mut expected_challenges: Vec<Uuid> = Vec::new();

        for _ in 0..challenges_per_user {
            let cid = seed_challenge(&pool, *uid).await;
            expected_challenges.push(cid);
            for _ in 0..racers_per_challenge {
                // ~60% correct, ~40% wrong
                let answer = if rand::random::<f64>() < 0.6 { 42i64 } else { 999i64 };
                entries.push((cid, answer));
            }
        }

        let pool = pool.clone();
        let sub = sub.clone();
        let uid = *uid;
        let handle = tokio::spawn(async move {
            let results = concurrent_mint(&pool, &sub, entries).await;
            (uid, expected_challenges, results)
        });
        all_handles.push(handle);
    }

    // Collect all results
    let mut all_per_user: Vec<(i64, Vec<Uuid>, Vec<MintResult>)> = Vec::new();
    for h in all_handles {
        all_per_user.push(h.await.unwrap());
    }

    // INVARIANT 1: per-challenge exactly-once holds for ALL challenges
    for (uid, cids, _results) in &all_per_user {
        for cid in cids {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM earnings WHERE challenge_id = $1",
            )
            .bind(cid)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert!(
                count.0 <= 1,
                "user {uid} challenge {cid}: at most 1 earnings row, got {}",
                count.0
            );

            let status: (String,) =
                sqlx::query_as("SELECT status FROM challenges WHERE id = $1")
                    .bind(cid)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert!(
                status.0 == "CLAIMED" || status.0 == "EXPIRED",
                "challenge {cid} terminal state violation: {s}",
                s = status.0
            );
        }
    }

    // INVARIANT 2: no cross-user leakage — each user's balance = their own CLAIMED rewards
    for (uid, cids, _results) in &all_per_user {
        let balance: (Option<i64>,) = sqlx::query_as(
            "SELECT SUM(amount)::BIGINT FROM earnings WHERE user_id = $1",
        )
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();
        let actual = balance.0.unwrap_or(0);

        let expected: (Option<i64>,) = sqlx::query_as(
            "SELECT COALESCE(SUM(reward), 0)::BIGINT FROM challenges
             WHERE user_id = $1 AND status = 'CLAIMED'",
        )
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(
            actual,
            expected.0.unwrap_or(0),
            "user {uid}: balance {actual} != expected {exp} from CLAIMED challenges",
            exp = expected.0.unwrap_or(0)
        );

        // INVARIANT 3: balance never exceeds total possible for this user
        let total_possible: (Option<i64>,) = sqlx::query_as(
            "SELECT COALESCE(SUM(reward), 0)::BIGINT FROM challenges WHERE user_id = $1",
        )
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(
            actual <= total_possible.0.unwrap_or(0),
            "user {uid}: balance {actual} > total possible {}",
            total_possible.0.unwrap_or(0)
        );
    }

    // INVARIANT 4: no earnings rows reference non-existent users (FK guarantees this at DB level)
    let orphan_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM earnings e LEFT JOIN users u ON u.id = e.user_id WHERE u.id IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(orphan_count.0, 0, "found orphan earnings rows");
}

