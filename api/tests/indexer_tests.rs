/// Tests for the Claimed-event indexer: cursor + idempotency + crash-safety.
use mathcoin_api::chain::client::{ClaimedEvent, MockChainClient};
use mathcoin_api::chain::indexer::index_claimed_events;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

async fn test_pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://mathcoin:mathcoin@localhost:5432/mathcoin_test".into());
    PgPoolOptions::new().max_connections(2).connect(&url).await.unwrap()
}

async fn clean(pool: &PgPool) {
    sqlx::query("DELETE FROM distribution_entries").execute(pool).await.unwrap();
    sqlx::query("DELETE FROM distributions").execute(pool).await.unwrap();
    sqlx::query("DELETE FROM earnings").execute(pool).await.unwrap();
    sqlx::query("DELETE FROM challenges").execute(pool).await.unwrap();
    sqlx::query("DELETE FROM users").execute(pool).await.unwrap();
    sqlx::query("DELETE FROM indexer_state").execute(pool).await.unwrap();
}

async fn seed_user_with_address(pool: &PgPool, sub: &str, addr: &str) -> i64 {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO users (provider_sub, email, claim_address) VALUES ($1, $2, $3)
         ON CONFLICT (provider_sub) DO UPDATE SET claim_address = EXCLUDED.claim_address
         RETURNING id",
    )
    .bind(sub).bind(format!("{sub}@test.com")).bind(addr)
    .fetch_one(pool).await.unwrap();
    row.0
}

#[tokio::test]
async fn indexer_processes_events_updates_cache_and_advances_cursor() {
    let pool = test_pool().await;
    clean(&pool).await;

    seed_user_with_address(&pool, "idx-a", "0xAAA").await;
    seed_user_with_address(&pool, "idx-b", "0xBBB").await;

    let client = MockChainClient::new();
    client.add_event(ClaimedEvent { account: "0xAAA".into(), cumulative_amount: 100, block_number: 50, tx_hash: "0x1".into() }).await;
    client.add_event(ClaimedEvent { account: "0xBBB".into(), cumulative_amount: 200, block_number: 51, tx_hash: "0x2".into() }).await;

    index_claimed_events(&pool, &client, 10).await.unwrap();

    // Cache updated
    let claimed_a: (i64,) = sqlx::query_as("SELECT claimed_onchain FROM users WHERE claim_address = '0xAAA'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(claimed_a.0, 100);

    let claimed_b: (i64,) = sqlx::query_as("SELECT claimed_onchain FROM users WHERE claim_address = '0xBBB'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(claimed_b.0, 200);

    // Cursor advanced
    let cursor: (i64,) = sqlx::query_as("SELECT last_processed_block FROM indexer_state WHERE key = 'claimed_events'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(cursor.0, 51);
}

#[tokio::test]
async fn indexer_idempotent_replay_no_double_effect() {
    let pool = test_pool().await;
    clean(&pool).await;

    seed_user_with_address(&pool, "idx-c", "0xCCC").await;

    let client = MockChainClient::new();
    client.add_event(ClaimedEvent { account: "0xCCC".into(), cumulative_amount: 77, block_number: 100, tx_hash: "0x3".into() }).await;

    // First run
    index_claimed_events(&pool, &client, 10).await.unwrap();

    // Add more events for second run (so from_block is > cursor)
    client.add_event(ClaimedEvent { account: "0xCCC".into(), cumulative_amount: 77, block_number: 200, tx_hash: "0x4".into() }).await;

    // Second run — re-processes 77 at block 200, value unchanged
    index_claimed_events(&pool, &client, 10).await.unwrap();

    let claimed: (i64,) = sqlx::query_as("SELECT claimed_onchain FROM users WHERE claim_address = '0xCCC'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(claimed.0, 77, "replay must not double-count");
}

#[tokio::test]
async fn indexer_crash_before_cursor_still_reprocesses() {
    let pool = test_pool().await;
    clean(&pool).await;

    seed_user_with_address(&pool, "idx-d", "0xDDD").await;

    // Insert a cursor value (simulating progress)
    sqlx::query("INSERT INTO indexer_state (key, last_processed_block) VALUES ('claimed_events', 10)")
        .execute(&pool).await.unwrap();

    let client = MockChainClient::new();
    // Events at blocks > 10 will be processed
    client.add_event(ClaimedEvent { account: "0xDDD".into(), cumulative_amount: 55, block_number: 11, tx_hash: "0x5".into() }).await;
    client.add_event(ClaimedEvent { account: "0xDDD".into(), cumulative_amount: 55, block_number: 30, tx_hash: "0x6".into() }).await;

    index_claimed_events(&pool, &client, 10).await.unwrap();

    // Cache should have 55 (cumulative, not 110)
    let claimed: (i64,) = sqlx::query_as("SELECT claimed_onchain FROM users WHERE claim_address = '0xDDD'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(claimed.0, 55);
}

#[tokio::test]
async fn self_submitted_claim_reflected_only_via_indexer() {
    let pool = test_pool().await;
    clean(&pool).await;

    seed_user_with_address(&pool, "idx-e", "0xEEE").await;

    // User submits claim themselves (no /api/claim call)
    // Simulated by the indexer finding a Claimed event for them
    let client = MockChainClient::new();
    client.add_event(ClaimedEvent { account: "0xEEE".into(), cumulative_amount: 300, block_number: 42, tx_hash: "0x7".into() }).await;

    // Before indexer runs, claimed_onchain is 0
    let before: (i64,) = sqlx::query_as("SELECT claimed_onchain FROM users WHERE claim_address = '0xEEE'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(before.0, 0);

    index_claimed_events(&pool, &client, 10).await.unwrap();

    // After indexer, claimed_onchain reflects the self-submitted claim
    let after: (i64,) = sqlx::query_as("SELECT claimed_onchain FROM users WHERE claim_address = '0xEEE'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(after.0, 300, "self-submitted claim must be found by indexer");
}
