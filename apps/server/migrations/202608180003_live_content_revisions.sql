CREATE TABLE live_content_revisions (
  revision BIGINT PRIMARY KEY CHECK (revision > 0),
  schema_version INTEGER NOT NULL CHECK (schema_version > 0),
  activate_at TIMESTAMPTZ NOT NULL,
  payload JSONB NOT NULL,
  operator_id VARCHAR(64) NOT NULL,
  change_note VARCHAR(256) NOT NULL,
  rolled_back_from_revision BIGINT NULL CHECK (rolled_back_from_revision >= 0),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT live_content_operator_not_blank CHECK (length(trim(operator_id)) > 0),
  CONSTRAINT live_content_change_note_not_blank CHECK (length(trim(change_note)) >= 8),
  CONSTRAINT live_content_payload_object CHECK (jsonb_typeof(payload) = 'object')
);

CREATE INDEX live_content_activation_idx
  ON live_content_revisions (activate_at, revision DESC);
