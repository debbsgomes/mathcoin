use sqlx::PgPool;
use std::sync::Arc;
use tracing::{info, error};

use crate::chain::client::{ChainClient, ClaimedEvent};

/// Number of blocks to stay behind the chain tip to avoid indexing
/// blocks that may be reorged out (L2 reorgs are rare but possible).
const REORG_SAFE_DEPTH: u64 = 10;

/// Process Claimed events from the contract and update the claimed_onchain cache.
/// Idempotent: events carry cumulative values, so replay is safe.
/// Crash-safe: cursor advanced only after side effects are committed.
/// Reorg-safe: cursor stays REORG_SAFE_DEPTH blocks behind chain tip.
pub async fn index_claimed_events(
    pool: &PgPool,
    client: &dyn ChainClient,
    batch_size: u64,
) -> Result<u64, String> {
    let cursor: (i64,) = sqlx::query_as(
        "SELECT last_processed_block FROM indexer_state WHERE key = 'claimed_events'",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .unwrap_or((0,));

    let from_block = (cursor.0 as u64).saturating_add(1);

    let chain_tip = client.get_latest_block().await.map_err(|e| {
        error!(error = %e, "failed to fetch chain tip");
        e
    })?;

    let safe_limit = chain_tip.saturating_sub(REORG_SAFE_DEPTH);
    if from_block > safe_limit {
        info!(from_block, chain_tip, safe_limit, "no reorg-safe blocks to index");
        return Ok(cursor.0 as u64);
    }

    info!(from_block, chain_tip, safe_limit, "indexing Claimed events");

    let events = client.get_claim_events(from_block).await.map_err(|e| {
        error!(error = %e, "failed to fetch Claimed events");
        e
    })?;

    if events.is_empty() {
        return Ok(cursor.0 as u64);
    }

    let mut last_block = cursor.0 as u64;
    let mut processed = 0u64;

    for chunk in events.chunks(batch_size as usize) {
        for event in chunk {
            // Only process events from blocks at or below the safe limit
            if event.block_number > safe_limit {
                continue;
            }

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
