-- Phase 2: Off-chain earnings ledger — append-only.
-- UNIQUE(challenge_id) makes double-credit impossible (defense-in-depth).
CREATE TABLE IF NOT EXISTS earnings (
    id           BIGSERIAL PRIMARY KEY,
    user_id      BIGINT NOT NULL REFERENCES users(id),
    challenge_id UUID NOT NULL UNIQUE REFERENCES challenges(id),
    amount       BIGINT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_earnings_user ON earnings (user_id);
