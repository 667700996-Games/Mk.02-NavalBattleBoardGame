ALTER TABLE player_accounts
  ADD COLUMN leaderboard_visible BOOLEAN NOT NULL DEFAULT TRUE;

CREATE TABLE ranked_leaderboard_snapshots (
  id UUID PRIMARY KEY,
  season_id VARCHAR(32) NOT NULL,
  generated_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ NULL,
  archived BOOLEAN NOT NULL,
  CONSTRAINT ranked_leaderboard_snapshot_season_not_blank
    CHECK (length(trim(season_id)) >= 3),
  CONSTRAINT ranked_leaderboard_snapshot_lifetime CHECK (
    (archived AND expires_at IS NULL)
    OR (NOT archived AND expires_at IS NOT NULL AND expires_at > generated_at)
  )
);

CREATE UNIQUE INDEX ranked_leaderboard_archived_season_idx
  ON ranked_leaderboard_snapshots (season_id)
  WHERE archived;

CREATE INDEX ranked_leaderboard_snapshot_expiry_idx
  ON ranked_leaderboard_snapshots (expires_at)
  WHERE NOT archived;

CREATE TABLE ranked_leaderboard_snapshot_entries (
  snapshot_id UUID NOT NULL
    REFERENCES ranked_leaderboard_snapshots(id) ON DELETE CASCADE,
  rank INTEGER NOT NULL CHECK (rank > 0),
  account_id UUID NOT NULL REFERENCES player_accounts(id) ON DELETE CASCADE,
  rating INTEGER NOT NULL CHECK (rating BETWEEN 0 AND 4000),
  matches_played INTEGER NOT NULL CHECK (matches_played >= 5),
  wins INTEGER NOT NULL CHECK (wins >= 0),
  losses INTEGER NOT NULL CHECK (losses >= 0),
  peak_rating INTEGER NOT NULL CHECK (peak_rating BETWEEN 0 AND 4000),
  PRIMARY KEY (snapshot_id, rank),
  UNIQUE (snapshot_id, account_id),
  CONSTRAINT ranked_leaderboard_entry_results_consistent
    CHECK (wins + losses = matches_played),
  CONSTRAINT ranked_leaderboard_entry_peak_consistent CHECK (peak_rating >= rating)
);

CREATE INDEX ranked_leaderboard_entry_page_idx
  ON ranked_leaderboard_snapshot_entries (snapshot_id, rank);

CREATE TABLE ranked_leaderboard_cursors (
  id UUID PRIMARY KEY,
  snapshot_id UUID NOT NULL
    REFERENCES ranked_leaderboard_snapshots(id) ON DELETE CASCADE,
  after_rank INTEGER NOT NULL CHECK (after_rank > 0),
  expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX ranked_leaderboard_cursor_expiry_idx
  ON ranked_leaderboard_cursors (expires_at);
