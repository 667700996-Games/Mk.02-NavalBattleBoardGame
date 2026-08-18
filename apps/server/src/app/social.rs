use super::*;

impl AppState {
    pub async fn social_overview(
        &self,
        session: &UserSession,
    ) -> Result<SocialOverview, GameError> {
        let account_id = session.account_id.ok_or(GameError::SocialAccountRequired)?;
        let privacy = self.store.social_privacy(account_id).await?;
        let mut relationships = self.store.social_relationships(account_id).await?;
        let now = Utc::now();
        for relationship in &mut relationships {
            relationship.presence = SocialPresence::Offline;
            relationship.current_room_id = None;
            if relationship.friend_state == SocialFriendState::Friend
                && !relationship.blocked
                && self
                    .store
                    .social_privacy(relationship.target_identity_id)
                    .await
                    .is_ok_and(|privacy| privacy.show_presence)
            {
                let (presence, room_id) = self
                    .store
                    .social_presence(relationship.target_identity_id, now)
                    .await?;
                relationship.presence = presence;
                relationship.current_room_id = room_id;
            }
        }
        let mut recent_players = self.store.recent_players(account_id, 20).await?;
        for player in &mut recent_players {
            if let Some(relationship) = relationships
                .iter()
                .find(|relationship| relationship.target_identity_id == player.account_id)
            {
                player.friend = relationship.friend_state == SocialFriendState::Friend;
                player.muted = relationship.muted;
                player.blocked = relationship.blocked;
            }
        }
        Ok(SocialOverview {
            privacy,
            relationships,
            recent_players,
        })
    }

    pub async fn update_social_privacy(
        &self,
        session: &UserSession,
        allow_friend_requests: bool,
        show_presence: bool,
        allow_game_invites: bool,
    ) -> Result<SocialOverview, GameError> {
        let account_id = session.account_id.ok_or(GameError::SocialAccountRequired)?;
        self.store
            .set_social_privacy(
                account_id,
                SocialPrivacy {
                    allow_friend_requests,
                    show_presence,
                    allow_game_invites,
                    updated_at: Utc::now(),
                },
            )
            .await?;
        self.social_overview(session).await
    }
}
