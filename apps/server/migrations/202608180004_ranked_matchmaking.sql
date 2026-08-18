CREATE TABLE ranked_ratings (
  account_id UUID PRIMARY KEY REFERENCES player_accounts(id) ON DELETE CASCADE,
  rating INTEGER NOT NULL DEFAULT 1500 CHECK (rating BETWEEN 0 AND 4000),
  matches_played INTEGER NOT NULL DEFAULT 0 CHECK (matches_played >= 0),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE matchmaking_queue
  ADD COLUMN pool VARCHAR(16) NOT NULL DEFAULT 'CASUAL',
  ADD COLUMN region VARCHAR(32) NOT NULL DEFAULT 'AUTO',
  ADD COLUMN latency_ms INTEGER NOT NULL DEFAULT 0,
  ADD COLUMN rating INTEGER NULL,
  ADD COLUMN party_id UUID NULL,
  ADD COLUMN party_size SMALLINT NOT NULL DEFAULT 1,
  ADD CONSTRAINT matchmaking_pool CHECK (pool IN ('CASUAL', 'RANKED')),
  ADD CONSTRAINT matchmaking_region CHECK (
    region IN (
      'AUTO',
      'KOREA',
      'JAPAN',
      'SOUTHEAST_ASIA',
      'NORTH_AMERICA_WEST',
      'NORTH_AMERICA_EAST',
      'EUROPE'
    )
  ),
  ADD CONSTRAINT matchmaking_latency CHECK (latency_ms BETWEEN 0 AND 300),
  ADD CONSTRAINT matchmaking_rating CHECK (rating IS NULL OR rating BETWEEN 0 AND 4000),
  ADD CONSTRAINT matchmaking_party_size CHECK (party_size = 1);

ALTER TABLE matchmaking_queue
  ADD CONSTRAINT matchmaking_profile CHECK (
    (
      pool = 'CASUAL'
      AND region = 'AUTO'
      AND latency_ms = 0
      AND rating IS NULL
      AND (party_id IS NULL OR party_id = session_id)
    )
    OR (
      pool = 'RANKED'
      AND region <> 'AUTO'
      AND latency_ms BETWEEN 1 AND 300
      AND rating BETWEEN 0 AND 4000
      AND party_id IS NOT NULL
    )
  );

CREATE INDEX matchmaking_ranked_available_idx
  ON matchmaking_queue (pool, region, rating, queued_at)
  WHERE claim_id IS NULL;
