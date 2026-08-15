CREATE TABLE player_accounts (
  id UUID PRIMARY KEY,
  handle VARCHAR(32) NOT NULL,
  recovery_key_hash CHAR(64) NOT NULL UNIQUE,
  created_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT player_accounts_handle_length CHECK (char_length(handle) BETWEEN 2 AND 16)
);

CREATE UNIQUE INDEX player_accounts_handle_unique_idx ON player_accounts (lower(handle));

ALTER TABLE user_sessions
  ADD COLUMN IF NOT EXISTS account_id UUID NULL REFERENCES player_accounts(id) ON DELETE SET NULL;

CREATE INDEX user_sessions_account_idx ON user_sessions (account_id, last_seen_at DESC)
  WHERE account_id IS NOT NULL;
