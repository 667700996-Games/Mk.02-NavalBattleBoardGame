CREATE TABLE player_support_actions (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES player_accounts(id) ON DELETE CASCADE,
    operator_id varchar(64) NOT NULL CHECK (length(btrim(operator_id)) BETWEEN 2 AND 64),
    action_type varchar(32) NOT NULL CHECK (
        action_type IN ('REVOKE_SESSION', 'REVOKE_ALL_SESSIONS')
    ),
    reason varchar(500) NOT NULL CHECK (length(btrim(reason)) BETWEEN 8 AND 500),
    target_session_id uuid,
    affected_session_ids uuid[] NOT NULL CHECK (cardinality(affected_session_ids) > 0),
    created_at timestamptz NOT NULL
);

CREATE INDEX player_support_actions_account_created_idx
    ON player_support_actions (account_id, created_at DESC, id DESC);

COMMENT ON TABLE player_support_actions IS
    'Append-only customer-support actions performed through authenticated product tooling.';

CREATE FUNCTION reject_player_support_action_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'player support action history is append-only';
END;
$$;

CREATE TRIGGER player_support_actions_immutable_update
BEFORE UPDATE ON player_support_actions
FOR EACH ROW EXECUTE FUNCTION reject_player_support_action_mutation();

CREATE TRIGGER player_support_actions_immutable_delete
BEFORE DELETE ON player_support_actions
FOR EACH ROW
WHEN (pg_trigger_depth() = 0)
EXECUTE FUNCTION reject_player_support_action_mutation();
