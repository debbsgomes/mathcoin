-- Phase 5: Per-address cumulative entries + Merkle proof per distribution.
CREATE TABLE IF NOT EXISTS distribution_entries (
    distribution_id   BIGINT NOT NULL REFERENCES distributions(id),
    wallet_address    TEXT   NOT NULL,
    cumulative_amount BIGINT NOT NULL,
    proof             JSONB  NOT NULL,
    PRIMARY KEY (distribution_id, wallet_address)
);
