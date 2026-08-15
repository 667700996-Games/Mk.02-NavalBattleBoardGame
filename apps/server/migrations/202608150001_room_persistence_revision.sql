ALTER TABLE game_rooms
  ADD COLUMN IF NOT EXISTS persistence_revision BIGINT NOT NULL DEFAULT 0;

ALTER TABLE game_rooms
  ADD CONSTRAINT game_rooms_persistence_revision_nonnegative
  CHECK (persistence_revision >= 0) NOT VALID;

ALTER TABLE game_rooms
  VALIDATE CONSTRAINT game_rooms_persistence_revision_nonnegative;
