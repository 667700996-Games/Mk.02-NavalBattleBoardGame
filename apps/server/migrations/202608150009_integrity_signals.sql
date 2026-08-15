CREATE TABLE integrity_signals (
  id UUID PRIMARY KEY,
  subject_identity_id UUID NOT NULL,
  room_id UUID NULL,
  kind VARCHAR(32) NOT NULL
    CHECK (kind IN ('IMPOSSIBLE_ORDER','AUTOMATION','COLLUSION','INTENTIONAL_STALLING')),
  severity SMALLINT NOT NULL CHECK (severity BETWEEN 1 AND 5),
  confidence DOUBLE PRECISION NOT NULL CHECK (confidence BETWEEN 0 AND 1),
  evidence JSONB NOT NULL,
  occurrences INTEGER NOT NULL DEFAULT 1 CHECK (occurrences > 0),
  first_observed_at TIMESTAMPTZ NOT NULL,
  last_observed_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX integrity_signals_queue_idx
  ON integrity_signals (severity DESC, last_observed_at DESC);

CREATE INDEX integrity_signals_subject_idx
  ON integrity_signals (subject_identity_id, last_observed_at DESC);

CREATE UNIQUE INDEX integrity_signals_room_dedup_idx
  ON integrity_signals (subject_identity_id, room_id, kind)
  WHERE room_id IS NOT NULL;
