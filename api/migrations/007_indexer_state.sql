-- Phase 5: Event indexer cursor for Claimed event reconciliation.
CREATE TABLE IF NOT EXISTS indexer_state (
    key                  TEXT PRIMARY KEY,
    last_processed_block BIGINT NOT NULL DEFAULT 0
);
