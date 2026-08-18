CREATE TABLE privacy_requests (
  id UUID PRIMARY KEY,
  subject_fingerprint CHAR(64) NOT NULL,
  request_type VARCHAR(16) NOT NULL CHECK (request_type IN ('EXPORT','DELETE')),
  status VARCHAR(16) NOT NULL CHECK (status IN ('COMPLETED','FAILED')),
  created_at TIMESTAMPTZ NOT NULL,
  completed_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX privacy_requests_audit_idx
  ON privacy_requests (request_type, completed_at DESC);
