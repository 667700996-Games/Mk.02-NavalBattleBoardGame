CREATE INDEX IF NOT EXISTS game_results_recent_pairing_idx
  ON game_results (finished_at DESC, room_id);

CREATE INDEX IF NOT EXISTS game_result_participants_account_room_idx
  ON game_result_participants (account_id, room_id)
  WHERE account_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS game_result_participants_session_room_idx
  ON game_result_participants (session_id, room_id);
