CREATE TABLE user_sessions (
  id UUID PRIMARY KEY,
  nickname VARCHAR(64) NOT NULL,
  token_hash CHAR(64) NOT NULL UNIQUE,
  created_at TIMESTAMPTZ NOT NULL,
  last_seen_at TIMESTAMPTZ NOT NULL,
  current_room_id UUID NULL
);

CREATE INDEX user_sessions_last_seen_idx ON user_sessions (last_seen_at);

CREATE TABLE game_rooms (
  id UUID PRIMARY KEY,
  code VARCHAR(8) NOT NULL UNIQUE,
  name VARCHAR(64) NOT NULL,
  visibility VARCHAR(16) NOT NULL CHECK (visibility IN ('PUBLIC', 'PRIVATE')),
  status VARCHAR(24) NOT NULL,
  snapshot JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX game_rooms_lobby_idx ON game_rooms (visibility, status, created_at DESC);

CREATE TABLE game_results (
  room_id UUID PRIMARY KEY REFERENCES game_rooms(id) ON DELETE CASCADE,
  room_name VARCHAR(64) NOT NULL,
  participant_session_ids UUID[] NOT NULL,
  result JSONB NOT NULL,
  finished_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX game_results_participants_idx ON game_results USING GIN (participant_session_ids);
CREATE INDEX game_results_finished_idx ON game_results (finished_at DESC);
