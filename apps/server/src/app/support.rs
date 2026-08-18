use super::*;

impl AppState {
    pub async fn support_account(
        &self,
        query: String,
    ) -> Result<SupportAccountSnapshot, GameError> {
        let query = query.trim();
        if query.chars().count() < 3
            || query.chars().count() > 64
            || query.chars().any(char::is_control)
        {
            return Err(GameError::InvalidRequest);
        }
        self.store
            .support_account(query)
            .await?
            .ok_or(GameError::SupportAccountNotFound)
    }

    pub async fn revoke_support_sessions(
        &self,
        account_id: Uuid,
        session_id: Option<Uuid>,
        operator_id: String,
        reason: String,
    ) -> Result<SupportAction, GameError> {
        let operator_id = operator_id.trim();
        let reason = reason.trim();
        if operator_id.chars().count() < 2
            || operator_id.chars().count() > 64
            || operator_id.chars().any(char::is_control)
            || reason.chars().count() < 8
            || reason.chars().count() > 500
            || reason
                .chars()
                .any(|character| character.is_control() && character != '\n' && character != '\t')
        {
            return Err(GameError::InvalidRequest);
        }
        let action = if session_id.is_some() {
            SupportActionKind::RevokeSession
        } else {
            SupportActionKind::RevokeAllSessions
        };
        self.store
            .revoke_account_sessions_for_support(&NewSupportAction {
                id: Uuid::new_v4(),
                account_id,
                operator_id: operator_id.to_string(),
                action,
                reason: reason.to_string(),
                target_session_id: session_id,
                created_at: Utc::now(),
            })
            .await
    }
}
