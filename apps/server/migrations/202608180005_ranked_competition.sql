ALTER TABLE ranked_ratings
  ADD COLUMN season_id VARCHAR(32) NOT NULL DEFAULT 'FOUNDERS_SEASON',
  ADD CONSTRAINT ranked_ratings_season_id_not_blank CHECK (length(trim(season_id)) >= 3);

CREATE TABLE ranked_season_standings (
  account_id UUID NOT NULL REFERENCES player_accounts(id) ON DELETE CASCADE,
  season_id VARCHAR(32) NOT NULL,
  rating INTEGER NOT NULL DEFAULT 1500 CHECK (rating BETWEEN 0 AND 4000),
  matches_played INTEGER NOT NULL DEFAULT 0 CHECK (matches_played >= 0),
  wins INTEGER NOT NULL DEFAULT 0 CHECK (wins >= 0),
  losses INTEGER NOT NULL DEFAULT 0 CHECK (losses >= 0),
  peak_rating INTEGER NOT NULL DEFAULT 1500 CHECK (peak_rating BETWEEN 0 AND 4000),
  last_match_at TIMESTAMPTZ NULL,
  decay_steps_applied INTEGER NOT NULL DEFAULT 0 CHECK (decay_steps_applied >= 0),
  season_reward_issued_at TIMESTAMPTZ NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (account_id, season_id),
  CONSTRAINT ranked_standing_season_id_not_blank CHECK (length(trim(season_id)) >= 3),
  CONSTRAINT ranked_standing_results_consistent CHECK (wins + losses <= matches_played),
  CONSTRAINT ranked_standing_peak_consistent CHECK (peak_rating >= rating)
);

INSERT INTO ranked_season_standings (
  account_id,
  season_id,
  rating,
  matches_played,
  peak_rating,
  updated_at
)
SELECT account_id, season_id, rating, matches_played, rating, updated_at
FROM ranked_ratings
ON CONFLICT (account_id, season_id) DO NOTHING;

CREATE INDEX ranked_standings_season_rank_idx
  ON ranked_season_standings (season_id, rating DESC, matches_played DESC, account_id);

CREATE TABLE ranked_match_settlements (
  room_id UUID PRIMARY KEY REFERENCES game_results(room_id) ON DELETE CASCADE,
  season_id VARCHAR(32) NOT NULL,
  settled_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT ranked_settlement_season_id_not_blank CHECK (length(trim(season_id)) >= 3)
);

CREATE TABLE ranked_match_participants (
  room_id UUID NOT NULL REFERENCES ranked_match_settlements(room_id) ON DELETE CASCADE,
  account_id UUID NOT NULL REFERENCES player_accounts(id) ON DELETE CASCADE,
  outcome VARCHAR(8) NOT NULL CHECK (outcome IN ('WIN', 'LOSS')),
  rating_before INTEGER NOT NULL CHECK (rating_before BETWEEN 0 AND 4000),
  rating_after INTEGER NOT NULL CHECK (rating_after BETWEEN 0 AND 4000),
  rating_delta INTEGER NOT NULL CHECK (rating_delta BETWEEN -128 AND 128),
  placement_completed BOOLEAN NOT NULL,
  PRIMARY KEY (room_id, account_id)
);

CREATE INDEX ranked_match_participants_account_idx
  ON ranked_match_participants (account_id, room_id);

CREATE TABLE ranked_reward_ledger (
  id UUID PRIMARY KEY,
  account_id UUID NOT NULL REFERENCES player_accounts(id) ON DELETE CASCADE,
  source_kind VARCHAR(24) NOT NULL CHECK (
    source_kind IN ('RANKED_MATCH', 'RANKED_PLACEMENT', 'RANKED_SEASON')
  ),
  source_id VARCHAR(64) NOT NULL,
  season_id VARCHAR(32) NOT NULL,
  xp INTEGER NOT NULL CHECK (xp > 0),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (account_id, source_kind, source_id, season_id),
  CONSTRAINT ranked_reward_season_id_not_blank CHECK (length(trim(season_id)) >= 3)
);

CREATE INDEX ranked_reward_account_idx
  ON ranked_reward_ledger (account_id, created_at DESC);

ALTER TABLE matchmaking_queue
  ADD COLUMN season_key UUID NULL;

CREATE INDEX matchmaking_ranked_season_available_idx
  ON matchmaking_queue (pool, season_key, region, rating, queued_at)
  WHERE claim_id IS NULL AND pool = 'RANKED' AND season_key IS NOT NULL;
