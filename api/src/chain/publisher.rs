use sqlx::PgPool;
use std::sync::Arc;
use tracing::info;

use crate::chain::tx_submitter::{Transaction, TxProvider, TxSubmitter};

/// Poll for pending distributions and publish the oldest one on-chain.
/// Called periodically (e.g., every 30s).
/// Returns the tx_hash if a distribution was published, or None if nothing to publish.
pub async fn publish_pending_distribution<P: TxProvider>(
    pool: &PgPool,
    submitter: &TxSubmitter<P>,
) -> Result<Option<String>, String> {
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
        None => return Ok(None),
    };

    // Build the updateRoot transaction
    // In production, this would be a contract call to updateRoot(bytes32 root)
    let tx = Transaction {
        to: "0xContract".into(),
        data: root.as_bytes().to_vec(),
        value: 0,
        gas_limit: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
    };

    let receipt = submitter.submit(tx).await.map_err(|e| {
        tracing::warn!(error = %e, dist_id = dist_id, "updateRoot submission failed");
        e
    })?;

    if !receipt.success {
        tracing::warn!(dist_id = dist_id, tx_hash = %receipt.tx_hash, "updateRoot reverted");
        return Err("transaction reverted".into());
    }

    sqlx::query(
        "UPDATE distributions SET status = 'published', tx_hash = $1 WHERE id = $2",
    )
    .bind(&receipt.tx_hash)
    .bind(dist_id)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, dist_id = dist_id, "failed to mark distribution as published");
        e.to_string()
    })?;

    info!(dist_id = dist_id, root = %root, tx_hash = %receipt.tx_hash, "distribution published");

    Ok(Some(receipt.tx_hash))
}
