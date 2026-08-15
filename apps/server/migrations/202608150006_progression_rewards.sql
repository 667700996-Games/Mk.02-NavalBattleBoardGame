CREATE TABLE progression_reward_ledger (
  id UUID PRIMARY KEY,
  account_id UUID NOT NULL REFERENCES player_accounts(id) ON DELETE CASCADE,
  source_kind VARCHAR(24) NOT NULL CHECK (source_kind IN ('MISSION')),
  source_id VARCHAR(64) NOT NULL,
  period_key VARCHAR(32) NOT NULL,
  xp INTEGER NOT NULL CHECK (xp > 0),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  reversed_at TIMESTAMPTZ NULL,
  reversal_reason VARCHAR(256) NULL,
  CONSTRAINT progression_reward_reversal_consistent CHECK (
    (reversed_at IS NULL AND reversal_reason IS NULL)
    OR (reversed_at IS NOT NULL AND reversal_reason IS NOT NULL)
  ),
  UNIQUE (account_id, source_kind, source_id, period_key)
);

CREATE INDEX progression_reward_account_idx
  ON progression_reward_ledger (account_id, created_at DESC);

