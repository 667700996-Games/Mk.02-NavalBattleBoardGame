CREATE TABLE privacy_deletion_tombstones (
  account_id UUID PRIMARY KEY,
  request_id UUID NOT NULL UNIQUE,
  subject_fingerprint CHAR(64) NOT NULL,
  deleted_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX privacy_deletion_tombstones_deleted_idx
  ON privacy_deletion_tombstones (deleted_at);
