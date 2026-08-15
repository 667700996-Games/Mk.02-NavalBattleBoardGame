ALTER TABLE game_rooms
  ADD COLUMN IF NOT EXISTS authority_owner_id UUID NULL,
  ADD COLUMN IF NOT EXISTS authority_fencing_token BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS authority_lease_expires_at TIMESTAMPTZ NULL;

ALTER TABLE game_rooms
  ADD CONSTRAINT game_rooms_authority_fencing_token_nonnegative
  CHECK (authority_fencing_token >= 0) NOT VALID;

ALTER TABLE game_rooms
  VALIDATE CONSTRAINT game_rooms_authority_fencing_token_nonnegative;

CREATE INDEX game_rooms_expired_authority_idx
  ON game_rooms (authority_lease_expires_at)
  WHERE authority_owner_id IS NOT NULL;
