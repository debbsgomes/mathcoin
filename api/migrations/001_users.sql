-- Phase 1: Auth — users table.
-- Later phases add: claim_address (Phase 5), challenges, earnings, etc.

CREATE TABLE IF NOT EXISTS users (
    id            BIGSERIAL PRIMARY KEY,
    provider_sub  TEXT NOT NULL UNIQUE,
    email         TEXT UNIQUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
