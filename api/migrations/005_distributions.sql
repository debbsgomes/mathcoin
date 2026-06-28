-- Phase 5: Merkle root distributions published to the contract.
CREATE TABLE IF NOT EXISTS distributions (
    id           BIGSERIAL PRIMARY KEY,
    merkle_root  TEXT NOT NULL,
    tx_hash      TEXT,
    total_amount BIGINT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending_publish'
                 CHECK (status IN ('pending_publish', 'published')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
