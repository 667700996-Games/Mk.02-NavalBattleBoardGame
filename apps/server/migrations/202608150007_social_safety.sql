CREATE TABLE player_relationships (
  actor_identity_id UUID NOT NULL,
  target_identity_id UUID NOT NULL,
  target_nickname VARCHAR(64) NOT NULL,
  muted BOOLEAN NOT NULL DEFAULT false,
  blocked BOOLEAN NOT NULL DEFAULT false,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (actor_identity_id, target_identity_id),
  CONSTRAINT player_relationship_not_self CHECK (actor_identity_id <> target_identity_id),
  CONSTRAINT player_relationship_has_effect CHECK (muted OR blocked)
);

CREATE INDEX player_relationship_target_idx
  ON player_relationships (target_identity_id, actor_identity_id)
  WHERE blocked;

CREATE TABLE player_reports (
  id UUID PRIMARY KEY,
  reporter_identity_id UUID NOT NULL,
  target_identity_id UUID NOT NULL,
  room_id UUID NULL,
  target_player_id UUID NULL,
  target_nickname VARCHAR(64) NOT NULL,
  category VARCHAR(32) NOT NULL CHECK (category IN ('CHAT','NAME','CHEATING','STALLING','OTHER')),
  details VARCHAR(1000) NOT NULL,
  evidence JSONB NOT NULL,
  status VARCHAR(24) NOT NULL DEFAULT 'OPEN' CHECK (status IN ('OPEN','REVIEWING','ACTIONED','DISMISSED')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT player_report_not_self CHECK (reporter_identity_id <> target_identity_id)
);

CREATE INDEX player_reports_queue_idx ON player_reports (status, created_at);

