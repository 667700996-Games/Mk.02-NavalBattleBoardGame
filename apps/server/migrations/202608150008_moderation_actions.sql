CREATE TABLE player_moderation_actions (
  id UUID PRIMARY KEY,
  report_id UUID NOT NULL REFERENCES player_reports(id) ON DELETE CASCADE,
  target_identity_id UUID NOT NULL,
  operator_id VARCHAR(64) NOT NULL,
  action_type VARCHAR(24) NOT NULL
    CHECK (action_type IN ('WARN','SUSPEND','BAN','DISMISS','REVERSE')),
  reason VARCHAR(1000) NOT NULL,
  expires_at TIMESTAMPTZ NULL,
  reverses_action_id UUID NULL REFERENCES player_moderation_actions(id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT moderation_action_expiry CHECK (
    (action_type = 'SUSPEND' AND expires_at IS NOT NULL)
    OR (action_type <> 'SUSPEND' AND expires_at IS NULL)
  ),
  CONSTRAINT moderation_action_reversal CHECK (
    (action_type = 'REVERSE' AND reverses_action_id IS NOT NULL)
    OR (action_type <> 'REVERSE' AND reverses_action_id IS NULL)
  )
);

CREATE UNIQUE INDEX player_moderation_single_reversal_idx
  ON player_moderation_actions (reverses_action_id)
  WHERE reverses_action_id IS NOT NULL;

CREATE INDEX player_moderation_target_idx
  ON player_moderation_actions (target_identity_id, created_at DESC);

CREATE INDEX player_moderation_report_idx
  ON player_moderation_actions (report_id, created_at);
