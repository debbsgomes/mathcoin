-- Phase 5: Add claim_address + claimed_onchain to users.
ALTER TABLE users ADD COLUMN IF NOT EXISTS claim_address TEXT UNIQUE;
ALTER TABLE users ADD COLUMN IF NOT EXISTS claimed_onchain BIGINT NOT NULL DEFAULT 0;
