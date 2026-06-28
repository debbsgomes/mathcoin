-- Phase 2: Challenges — ephemeral game state.
-- Lifecycle: PENDING -> CLAIMED | EXPIRED.
CREATE TABLE IF NOT EXISTS challenges (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     BIGINT NOT NULL REFERENCES users(id),
    problem     TEXT NOT NULL,
    solution    BIGINT NOT NULL,
    difficulty  SMALLINT NOT NULL DEFAULT 1,
    reward      BIGINT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'PENDING'
                CHECK (status IN ('PENDING','CLAIMED','EXPIRED')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_challenges_user_status ON challenges (user_id, status);
