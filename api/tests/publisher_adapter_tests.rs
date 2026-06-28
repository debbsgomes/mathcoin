/// Tests for the publish path: pending distributions → updateRoot → published.
use mathcoin_api::chain::tx_submitter::{
    MockTxProvider, Transaction, TxSubmitter, TxReceipt,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

async fn test_pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://mathcoin:mathcoin@localhost:5432/mathcoin_test".into());
    PgPoolOptions::new().max_connections(2).connect(&url).await.unwrap()
}

async fn clean(pool: &PgPool) {
    sqlx::query("DELETE FROM distribution_entries").execute(pool).await.unwrap();
    sqlx::query("DELETE FROM distributions").execute(pool).await.unwrap();
}

/// Insert a pending distribution with entries, return (distribution_id, root).
async fn seed_pending(pool: &PgPool, root: &str, addresses: &[&str]) -> i64 {
    let mut tx = pool.begin().await.unwrap();
    let dist_id: (i64,) = sqlx::query_as(
        "INSERT INTO distributions (merkle_root, total_amount, status)
         VALUES ($1, 100, 'pending_publish')
         RETURNING id",
    )
    .bind(root)
    .fetch_one(&mut *tx)
    .await
    .unwrap();

    for addr in addresses {
        sqlx::query(
            "INSERT INTO distribution_entries (distribution_id, wallet_address, cumulative_amount, proof)
             VALUES ($1, $2, 50, '[]'::jsonb)",
        )
        .bind(dist_id.0)
        .bind(addr)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    dist_id.0
}

/// Publish the oldest pending distribution using the TxSubmitter.
/// Returns the tx_hash on success.
async fn publish_next(pool: &PgPool, submitter: &TxSubmitter<MockTxProvider>) -> Result<String, String> {
    let pending: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, merkle_root FROM distributions
         WHERE status = 'pending_publish'
         ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    let (dist_id, root) = match pending {
        Some(r) => r,
        None => return Err("no pending distributions".into()),
    };

    // Build and submit the updateRoot transaction
    let tx = Transaction {
        to: "0xContract".into(),
        data: root.as_bytes().to_vec(),
        value: 0,
        gas_limit: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
    };

    let receipt = submitter.submit(tx).await?;
    assert!(receipt.success, "tx must succeed");

    sqlx::query(
        "UPDATE distributions SET status = 'published', tx_hash = $1 WHERE id = $2",
    )
    .bind(&receipt.tx_hash)
    .bind(dist_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(receipt.tx_hash)
}

// ---- Tests ----

#[tokio::test]
async fn pending_distribution_becomes_published_on_confirmation() {
    let pool = test_pool().await;
    clean(&pool).await;

    let provider = Arc::new(MockTxProvider::new(0));
    // Pre-seed receipt so the submission confirms immediately
    provider.add_receipt(
        &format!("0x{:064x}", 0u64),
        TxReceipt { tx_hash: format!("0x{:064x}", 0u64), success: true, block_number: 100 },
    ).await;

    let submitter = TxSubmitter::new(provider.clone(), "0xPublisher".into()).await.unwrap()
        .with_confirmation_timeout(Duration::from_secs(2));

    let dist_id = seed_pending(&pool, "0xdeadbeef", &["0xAbc"]).await;

    let tx_hash = publish_next(&pool, &submitter).await.unwrap();

    // Verify the distribution is now published
    let status: (String,) = sqlx::query_as(
        "SELECT status FROM distributions WHERE id = $1",
    )
    .bind(dist_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status.0, "published");

    let stored_hash: (String,) = sqlx::query_as(
        "SELECT tx_hash FROM distributions WHERE id = $1",
    )
    .bind(dist_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored_hash.0, tx_hash);
}

#[tokio::test]
async fn multiple_pending_publish_in_created_at_order() {
    let pool = test_pool().await;
    clean(&pool).await;

    let provider = Arc::new(MockTxProvider::new(0));
    for i in 0..3u64 {
        provider.add_receipt(
            &format!("0x{:064x}", i),
            TxReceipt { tx_hash: format!("0x{:064x}", i), success: true, block_number: 100 + i },
        ).await;
    }

    let submitter = TxSubmitter::new(provider, "0xPublisher".into()).await.unwrap()
        .with_confirmation_timeout(Duration::from_secs(2));

    // Seed 3 distributions in order
    let id1 = seed_pending(&pool, "0xroot1", &["0xA"]).await;
    let id2 = seed_pending(&pool, "0xroot2", &["0xB"]).await;
    let id3 = seed_pending(&pool, "0xroot3", &["0xC"]).await;

    // Publish all three
    for _ in 0..3 {
        publish_next(&pool, &submitter).await.unwrap();
    }

    // Verify all are published
    for id in &[id1, id2, id3] {
        let status: (String,) = sqlx::query_as(
            "SELECT status FROM distributions WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status.0, "published", "distribution {id} should be published");
    }
}

#[tokio::test]
async fn submission_failure_keeps_status_pending() {
    let pool = test_pool().await;
    clean(&pool).await;

    let provider = Arc::new(MockTxProvider::new(0));
    // Make the submission fail
    provider.set_fail_next("insufficient funds").await;

    let submitter = TxSubmitter::new(provider, "0xPublisher".into()).await.unwrap()
        .with_confirmation_timeout(Duration::from_secs(2));

    let dist_id = seed_pending(&pool, "0xroot", &["0xA"]).await;

    let result = publish_next(&pool, &submitter).await;
    assert!(result.is_err());

    // Should still be pending_publish
    let status: (String,) = sqlx::query_as(
        "SELECT status FROM distributions WHERE id = $1",
    )
    .bind(dist_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status.0, "pending_publish", "failed publish must not mark as published");
}

#[tokio::test]
async fn status_only_flips_after_confirmation() {
    let pool = test_pool().await;
    clean(&pool).await;

    let provider = Arc::new(MockTxProvider::new(0));
    // Make receipt never arrive (timeout)
    provider.set_receipt_delay(Duration::from_millis(500)).await;

    let submitter = TxSubmitter::new(provider, "0xPublisher".into()).await.unwrap()
        .with_confirmation_timeout(Duration::from_millis(100));

    let dist_id = seed_pending(&pool, "0xroot", &["0xA"]).await;

    let result = publish_next(&pool, &submitter).await;
    assert!(result.is_err(), "should timeout");

    // Must still be pending — never marked published without confirmation
    let status: (String,) = sqlx::query_as(
        "SELECT status FROM distributions WHERE id = $1",
    )
    .bind(dist_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status.0, "pending_publish", "status must not flip without confirmation");
}
