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
