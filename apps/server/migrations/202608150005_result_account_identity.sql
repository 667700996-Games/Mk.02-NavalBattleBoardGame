ALTER TABLE game_results
  ADD COLUMN IF NOT EXISTS participant_account_ids UUID[] NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS game_results_accounts_idx
  ON game_results USING GIN (participant_account_ids);

CREATE TABLE IF NOT EXISTS game_result_participants (
  room_id UUID NOT NULL REFERENCES game_results(room_id) ON DELETE CASCADE,
  player_id UUID NOT NULL,
  session_id UUID NOT NULL,
  account_id UUID NULL REFERENCES player_accounts(id) ON DELETE SET NULL,
  PRIMARY KEY (room_id, player_id)
);

CREATE INDEX IF NOT EXISTS game_result_participants_session_idx
  ON game_result_participants (session_id);

CREATE INDEX IF NOT EXISTS game_result_participants_account_idx
  ON game_result_participants (account_id)
  WHERE account_id IS NOT NULL;

INSERT INTO game_result_participants (room_id, player_id, session_id, account_id)
SELECT results.room_id,
       (player.value->>'id')::UUID,
       (player.value->>'sessionId')::UUID,
       sessions.account_id
FROM game_results results
JOIN game_rooms rooms ON rooms.id = results.room_id
CROSS JOIN LATERAL jsonb_array_elements(rooms.snapshot->'players') AS player(value)
LEFT JOIN user_sessions sessions ON sessions.id = (player.value->>'sessionId')::UUID
ON CONFLICT (room_id, player_id) DO NOTHING;

UPDATE game_results results
SET participant_account_ids = identities.account_ids
FROM (
  SELECT room_id, array_agg(DISTINCT account_id) FILTER (WHERE account_id IS NOT NULL) AS account_ids
  FROM game_result_participants
  GROUP BY room_id
) identities
WHERE results.room_id = identities.room_id
  AND identities.account_ids IS NOT NULL;
