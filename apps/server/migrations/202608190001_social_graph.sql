ALTER TABLE player_accounts
  ADD COLUMN allow_friend_requests BOOLEAN NOT NULL DEFAULT true,
  ADD COLUMN show_presence BOOLEAN NOT NULL DEFAULT true,
  ADD COLUMN allow_game_invites BOOLEAN NOT NULL DEFAULT true,
  ADD COLUMN social_privacy_updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE TABLE player_social_links (
  actor_account_id UUID NOT NULL REFERENCES player_accounts(id) ON DELETE CASCADE,
  target_account_id UUID NOT NULL REFERENCES player_accounts(id) ON DELETE CASCADE,
  target_handle VARCHAR(64) NOT NULL,
  friend_state VARCHAR(24) NOT NULL DEFAULT 'NONE'
    CHECK (friend_state IN ('NONE','OUTGOING','INCOMING','FRIEND')),
  friend_request_id UUID NULL,
  party_state VARCHAR(24) NOT NULL DEFAULT 'NONE'
    CHECK (party_state IN ('NONE','OUTGOING_INVITE','INCOMING_INVITE','OWNER','MEMBER')),
  party_id UUID NULL,
  game_invite JSONB NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (actor_account_id, target_account_id),
  CONSTRAINT player_social_link_not_self CHECK (actor_account_id <> target_account_id),
  CONSTRAINT player_social_link_friend_request CHECK (
    (friend_state IN ('OUTGOING','INCOMING') AND friend_request_id IS NOT NULL)
    OR (friend_state IN ('NONE','FRIEND') AND friend_request_id IS NULL)
  ),
  CONSTRAINT player_social_link_party_identity CHECK (
    (party_state = 'NONE' AND party_id IS NULL)
    OR (party_state <> 'NONE' AND party_id IS NOT NULL)
  ),
  CONSTRAINT player_social_link_game_invite_shape CHECK (
    game_invite IS NULL
    OR (
      jsonb_typeof(game_invite) = 'object'
      AND game_invite ?& ARRAY['id','direction','roomId','roomCode','roomName','expiresAt']
      AND pg_column_size(game_invite) <= 2048
    )
  ),
  CONSTRAINT player_social_link_has_effect CHECK (
    friend_state <> 'NONE' OR party_state <> 'NONE' OR game_invite IS NOT NULL
  )
);

CREATE INDEX player_social_link_friend_idx
  ON player_social_links (actor_account_id, updated_at DESC)
  WHERE friend_state = 'FRIEND';

CREATE INDEX player_social_link_party_idx
  ON player_social_links (party_id)
  WHERE party_id IS NOT NULL;

CREATE INDEX game_result_participants_recent_social_idx
  ON game_result_participants (account_id, room_id)
  WHERE account_id IS NOT NULL;
