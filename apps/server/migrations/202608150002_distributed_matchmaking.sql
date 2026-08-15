CREATE TABLE matchmaking_queue (
  session_id UUID PRIMARY KEY REFERENCES user_sessions(id) ON DELETE CASCADE,
  queued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  claim_id UUID NULL,
  claimed_at TIMESTAMPTZ NULL,
  CONSTRAINT matchmaking_claim_pair CHECK (
    (claim_id IS NULL AND claimed_at IS NULL)
    OR (claim_id IS NOT NULL AND claimed_at IS NOT NULL)
  )
);

CREATE INDEX matchmaking_queue_available_idx
  ON matchmaking_queue (queued_at)
  WHERE claim_id IS NULL;

CREATE INDEX matchmaking_queue_claim_idx
  ON matchmaking_queue (claim_id)
  WHERE claim_id IS NOT NULL;
