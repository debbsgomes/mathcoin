use sqlx::PgPool;
use std::sync::Arc;
use tracing::{info, warn, error};

use crate::chain::client::{ChainClient, ClaimedEvent};

/// Process Claimed events from the contract and update the claimed_onchain cache.
/// Idempotent: events carry cumulative values, so replay is safe.
/// Crash-safe: cursor advanced only after side effects are committed.
pub async fn index_claimed_events(
    pool: &PgPool,
    client: &dyn ChainClient,
    batch_size: u64,
) -> Result<u64, String> {
    // Read current cursor
    let cursor: (i64,) = sqlx::query_as(
        "SELECT last_processed_block FROM indexer_state WHERE key = 'claimed_events'",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .unwrap_or((0,));

    let from_block = (cursor.0 as u64).saturating_add(1);

    info!(from_block, "indexing Claimed events");

    let events = client.get_claim_events(from_block).await.map_err(|e| {
        error!(error = %e, "failed to fetch Claimed events");
        e
    })?;

    if events.is_empty() {
        return Ok(cursor.0 as u64);
    }

    let mut last_block = cursor.0 as u64;
    let mut processed = 0u64;

    // Process in batches for crash-safety
    for chunk in events.chunks(batch_size as usize) {
        for event in chunk {
            sqlx::query(
                "UPDATE users SET claimed_onchain = $1 WHERE claim_address = $2",
            )
            .bind(event.cumulative_amount as i64)
            .bind(&event.account)
            .execute(pool)
            .await
            .map_err(|e| {
                error!(error = %e, account = %event.account, "failed to update claimed_onchain cache");
                e.to_string()
            })?;

            last_block = last_block.max(event.block_number);
            processed += 1;
        }

        // Advance cursor after batch side effects are committed
        sqlx::query(
            "INSERT INTO indexer_state (key, last_processed_block) VALUES ('claimed_events', $1)
             ON CONFLICT (key) DO UPDATE SET last_processed_block = $1",
        )
        .bind(last_block as i64)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to advance cursor");
            e.to_string()
        })?;
    }

    info!(processed, last_block = last_block, "indexer batch complete");
    Ok(last_block)
}
