CREATE FUNCTION index_legacy_game_result_participants()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
  INSERT INTO game_result_participants (
    room_id,
    player_id,
    session_id,
    account_id
  )
  SELECT NEW.room_id,
         (player.value->>'id')::UUID,
         (player.value->>'sessionId')::UUID,
         session.account_id
  FROM game_rooms room
  CROSS JOIN LATERAL jsonb_array_elements(
    COALESCE(room.snapshot->'players', '[]'::JSONB)
  ) AS player(value)
  LEFT JOIN user_sessions session
    ON session.id = (player.value->>'sessionId')::UUID
  WHERE room.id = NEW.room_id
  ON CONFLICT (room_id, player_id) DO UPDATE
  SET session_id = EXCLUDED.session_id,
      account_id = EXCLUDED.account_id;

  UPDATE game_results result
  SET participant_account_ids = COALESCE(
    (
      SELECT array_agg(DISTINCT participant.account_id)
        FILTER (WHERE participant.account_id IS NOT NULL)
      FROM game_result_participants participant
      WHERE participant.room_id = NEW.room_id
    ),
    '{}'::UUID[]
  )
  WHERE result.room_id = NEW.room_id;

  RETURN NEW;
END;
$$;

CREATE TRIGGER game_results_legacy_participant_dual_write
AFTER INSERT ON game_results
FOR EACH ROW EXECUTE FUNCTION index_legacy_game_result_participants();
