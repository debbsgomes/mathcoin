/// DB-level constraint tests for challenges and earnings tables.
/// Proves the UNIQUE and CHECK constraints work at the database layer.
use sqlx::PgPool;

async fn clean(pool: &PgPool) {
    sqlx::query("DELETE FROM earnings").execute(pool).await.unwrap();
    sqlx::query("DELETE FROM challenges").execute(pool).await.unwrap();
    sqlx::query("DELETE FROM users").execute(pool).await.unwrap();
}

#[sqlx::test]
async fn migration_applies_cleanly(pool: PgPool) {
    // The tables exist — just verify we can insert a row
    sqlx::query("INSERT INTO users (provider_sub, email) VALUES ('cst-test', 'cst@test.com')")
        .execute(&pool)
        .await
        .unwrap();

    let challenge_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO challenges (user_id, problem, solution, difficulty, reward, expires_at)
         VALUES (1, '7 + 3', 10, 1, 5, now() + INTERVAL '5 minutes')
         RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO earnings (user_id, challenge_id, amount) VALUES (1, $1, 5)")
        .bind(challenge_id.0)
        .execute(&pool)
        .await
        .unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM earnings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1);
}

#[sqlx::test]
async fn duplicate_challenge_id_in_earnings_fails(pool: PgPool) {
    clean(&pool).await;
    sqlx::query("INSERT INTO users (provider_sub, email) VALUES ('cst-dup', 'dup@test.com')")
        .execute(&pool)
        .await
        .unwrap();

    let challenge_id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO challenges (user_id, problem, solution, difficulty, reward, expires_at)
         VALUES (1, '1 + 1', 2, 1, 5, now() + INTERVAL '5 minutes')
         RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    // First insert succeeds
    sqlx::query("INSERT INTO earnings (user_id, challenge_id, amount) VALUES (1, $1, 5)")
        .bind(challenge_id.0)
        .execute(&pool)
        .await
        .unwrap();

    // Second insert with same challenge_id MUST fail (UNIQUE constraint)
    let result = sqlx::query("INSERT INTO earnings (user_id, challenge_id, amount) VALUES (1, $1, 5)")
        .bind(challenge_id.0)
        .execute(&pool)
        .await;

    assert!(result.is_err(), "UNIQUE(challenge_id) should reject duplicate");
}

#[sqlx::test]
async fn invalid_challenge_status_fails(pool: PgPool) {
    clean(&pool).await;
    sqlx::query("INSERT INTO users (provider_sub, email) VALUES ('cst-status', 'status@test.com')")
        .execute(&pool)
        .await
        .unwrap();

    // Insert with invalid status 'SOLVED' (not in CHECK constraint)
    let result = sqlx::query(
        "INSERT INTO challenges (user_id, problem, solution, difficulty, reward, status, expires_at)
         VALUES (1, '1 + 1', 2, 1, 5, 'SOLVED', now() + INTERVAL '5 minutes')",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "CHECK(status) should reject 'SOLVED'");
}

// ---- Phase 5: On-chain data model ----

#[sqlx::test]
async fn migrations_phase5_apply_cleanly(pool: PgPool) {
    // Verify all new tables and columns exist
    sqlx::query("SELECT claim_address, claimed_onchain FROM users LIMIT 0")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO distributions (merkle_root, total_amount) VALUES ('0x00', 0)")
        .execute(&pool)
        .await
        .unwrap();

    let dist_id: (i64,) = sqlx::query_as("SELECT id FROM distributions LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO distribution_entries (distribution_id, wallet_address, cumulative_amount, proof)
         VALUES ($1, '0xAbc', 100, '[]'::jsonb)",
    )
    .bind(dist_id.0)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO indexer_state (key, last_processed_block) VALUES ('claimed_events', 0)")
        .execute(&pool)
        .await
        .unwrap();
}

#[sqlx::test]
async fn distribution_status_check_rejects_invalid(pool: PgPool) {
    let result = sqlx::query(
        "INSERT INTO distributions (merkle_root, total_amount, status)
         VALUES ('0x00', 0, 'INVALID')",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "CHECK(status) should reject 'INVALID'");
}

#[sqlx::test]
async fn distribution_status_accepts_valid(pool: PgPool) {
    sqlx::query(
        "INSERT INTO distributions (merkle_root, total_amount, status)
         VALUES ('0x00', 0, 'pending_publish')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO distributions (merkle_root, total_amount, status)
         VALUES ('0x01', 10, 'published')",
    )
    .execute(&pool)
    .await
    .unwrap();
}

#[sqlx::test]
async fn distribution_entries_composite_pk_rejects_duplicate(pool: PgPool) {
    sqlx::query("INSERT INTO distributions (merkle_root, total_amount) VALUES ('0x00', 0)")
        .execute(&pool)
        .await
        .unwrap();

    let dist_id: (i64,) = sqlx::query_as("SELECT id FROM distributions LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO distribution_entries (distribution_id, wallet_address, cumulative_amount, proof)
         VALUES ($1, '0xAbc', 100, '[]'::jsonb)",
    )
    .bind(dist_id.0)
    .execute(&pool)
    .await
    .unwrap();

    let result = sqlx::query(
        "INSERT INTO distribution_entries (distribution_id, wallet_address, cumulative_amount, proof)
         VALUES ($1, '0xAbc', 200, '[]'::jsonb)",
    )
    .bind(dist_id.0)
    .execute(&pool)
    .await;

    assert!(result.is_err(), "composite PK (distribution_id, wallet_address) should reject duplicate");
}

#[sqlx::test]
async fn claim_address_unique_rejects_duplicate(pool: PgPool) {
    sqlx::query("INSERT INTO users (provider_sub, email, claim_address) VALUES ('sub-a', 'a@test.com', '0xAAA')")
        .execute(&pool)
        .await
        .unwrap();

    let result = sqlx::query("INSERT INTO users (provider_sub, email, claim_address) VALUES ('sub-b', 'b@test.com', '0xAAA')")
        .execute(&pool)
        .await;

    assert!(result.is_err(), "UNIQUE(claim_address) should reject duplicate");
}
