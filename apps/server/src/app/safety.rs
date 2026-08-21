use super::accounts::constant_time_equal;
use super::*;

impl AppState {
    pub async fn safety_relationships(
        &self,
        session: &UserSession,
    ) -> Result<Vec<SafetyRelationship>, GameError> {
        self.store
            .safety_relationships(session.account_id.unwrap_or(session.id))
            .await
    }

    pub async fn update_safety_relationship(
        &self,
        session: &UserSession,
        room_id: Uuid,
        target_player_id: Uuid,
        muted: bool,
        blocked: bool,
    ) -> Result<SafetyRelationship, GameError> {
        let room = self.room(room_id).await?;
        let room = room.lock().await;
        let actor = room.player_for_session(session.id)?;
        let target = room
            .players
            .iter()
            .find(|player| player.id == target_player_id)
            .ok_or(GameError::InvalidRequest)?;
        if actor.id == target.id || target.kind == PlayerKind::Ai {
            return Err(GameError::InvalidRequest);
        }
        let target_identity_id = self
            .store
            .identity_for_session(target.session_id)
            .await?
            .ok_or(GameError::InvalidRequest)?;
        let actor_identity_id = session.account_id.unwrap_or(session.id);
        if actor_identity_id == target_identity_id {
            return Err(GameError::InvalidRequest);
        }
        let now = Utc::now();
        let mut relationship = self
            .store
            .safety_relationship_between(actor_identity_id, target_identity_id)
            .await?
            .unwrap_or_else(|| {
                SafetyRelationship::new(target_identity_id, target.nickname.clone(), now)
            });
        relationship.target_nickname = target.nickname.clone();
        relationship.muted = muted;
        relationship.blocked = blocked;
        relationship.updated_at = now;
        self.store
            .set_safety_relationship(actor_identity_id, relationship.clone())
            .await?;
        Ok(relationship)
    }

    pub async fn report_player(
        &self,
        session: &UserSession,
        room_id: Uuid,
        target_player_id: Uuid,
        category: ReportCategory,
        details: String,
    ) -> Result<PlayerReportReceipt, GameError> {
        let details = details.trim().to_string();
        if details.chars().count() < 4
            || details.chars().count() > 1000
            || details
                .chars()
                .any(|character| character.is_control() && character != '\n' && character != '\t')
        {
            return Err(GameError::InvalidRequest);
        }
        let room = self.room(room_id).await?;
        let room = room.lock().await;
        let reporter = room.player_for_session(session.id)?;
        let target = room
            .players
            .iter()
            .find(|player| player.id == target_player_id)
            .ok_or(GameError::InvalidRequest)?;
        if reporter.id == target.id || target.kind == PlayerKind::Ai {
            return Err(GameError::InvalidRequest);
        }
        let target_identity_id = self
            .store
            .identity_for_session(target.session_id)
            .await?
            .ok_or(GameError::InvalidRequest)?;
        let reporter_identity_id = session.account_id.unwrap_or(session.id);
        let created_at = Utc::now();
        let report_id = Uuid::new_v4();
        let evidence = serde_json::json!({
            "protocolVersion": crate::PROTOCOL_VERSION,
            "roomId": room.id,
            "roomVersion": room.version,
            "roomState": room.status,
            "reportedPlayerId": target.id,
            "reportedNickname": target.nickname.clone(),
            "messages": room.chat_messages.iter().rev().take(20).cloned().collect::<Vec<_>>(),
            "recentAttacks": room.game.as_ref().map(|game| game.attacks.iter().rev().take(20).cloned().collect::<Vec<_>>()).unwrap_or_default(),
            "capturedAt": created_at,
        });
        self.store
            .create_player_report(&NewPlayerReport {
                id: report_id,
                reporter_identity_id,
                target_identity_id,
                room_id,
                target_player_id,
                target_nickname: target.nickname.clone(),
                category,
                details,
                evidence,
                created_at,
            })
            .await?;
        Ok(PlayerReportReceipt {
            report_id,
            status: "OPEN",
            created_at,
        })
    }

    pub fn authorize_operator(&self, token: &str) -> Result<(), GameError> {
        let expected = self
            .settings
            .admin_token_hash
            .as_deref()
            .ok_or(GameError::Unauthorized)?;
        if constant_time_equal(hash_token(token).as_bytes(), expected.as_bytes()) {
            Ok(())
        } else {
            Err(GameError::Unauthorized)
        }
    }

    pub async fn moderation_cases(
        &self,
        search: Option<String>,
        status: Option<ReportStatus>,
        before: Option<chrono::DateTime<Utc>>,
        limit: Option<u32>,
    ) -> Result<ModerationCasePage, GameError> {
        let search = search
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if search.as_ref().is_some_and(|value| value.len() > 128) {
            return Err(GameError::InvalidRequest);
        }
        let limit = limit.unwrap_or(25).clamp(1, 100) as usize;
        self.store
            .moderation_cases(search.as_deref(), status, before, limit)
            .await
    }

    pub async fn integrity_signals(
        &self,
        search: Option<String>,
        kind: Option<IntegritySignalKind>,
        before: Option<chrono::DateTime<Utc>>,
        limit: Option<u32>,
    ) -> Result<IntegritySignalPage, GameError> {
        let search = search
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if search.as_ref().is_some_and(|value| value.len() > 128) {
            return Err(GameError::InvalidRequest);
        }
        self.store
            .integrity_signals(
                search.as_deref(),
                kind,
                before,
                limit.unwrap_or(25).clamp(1, 100) as usize,
            )
            .await
    }

    pub async fn record_integrity_signal(
        &self,
        session: &UserSession,
        room_id: Option<Uuid>,
        kind: IntegritySignalKind,
        severity: u8,
        confidence: f64,
        evidence: serde_json::Value,
    ) {
        if room_id.is_none() {
            let key = (session.id, kind);
            let now = std::time::Instant::now();
            if self
                .integrity_signal_cooldowns
                .get(&key)
                .is_some_and(|last| now.duration_since(*last) < Duration::from_secs(60))
            {
                return;
            }
            self.integrity_signal_cooldowns.insert(key, now);
        }
        let signal = NewIntegritySignal {
            id: Uuid::new_v4(),
            subject_identity_id: session.account_id.unwrap_or(session.id),
            room_id,
            kind,
            severity: severity.clamp(1, 5),
            confidence: confidence.clamp(0.0, 1.0),
            evidence,
            observed_at: Utc::now(),
        };
        match self.store.record_integrity_signal(&signal).await {
            Ok(stored) => {
                let metric = match kind {
                    IntegritySignalKind::ImpossibleOrder => {
                        &self.metrics.integrity_impossible_order
                    }
                    IntegritySignalKind::Automation => &self.metrics.integrity_automation,
                    IntegritySignalKind::Collusion => &self.metrics.integrity_collusion,
                    IntegritySignalKind::IntentionalStalling => &self.metrics.integrity_stalling,
                };
                metric.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    signal_id = %stored.id,
                    subject_identity_id = %stored.subject_identity_id,
                    signal_kind = kind.as_str(),
                    severity = stored.severity,
                    occurrences = stored.occurrences,
                    "game integrity signal recorded"
                );
            }
            Err(error) => tracing::error!(
                error_code = error.code(),
                signal_kind = kind.as_str(),
                "game integrity signal persistence failed"
            ),
        }
    }

    pub(super) async fn detect_finished_match_integrity(
        &self,
        room: &GameRoom,
    ) -> Result<(), GameError> {
        let Some(game) = room.game.as_ref() else {
            return Ok(());
        };
        let Some(result) = game.result.as_ref() else {
            return Ok(());
        };
        let human_players: Vec<_> = room
            .players
            .iter()
            .filter(|player| player.kind == PlayerKind::Human)
            .collect();
        for player in &human_players {
            let timeouts = game
                .total_timeout_counts
                .get(&player.id)
                .copied()
                .unwrap_or(0);
            if timeouts >= 3
                || (result.finish_reason == FinishReason::TurnTimeout
                    && result.loser_id == player.id)
            {
                let identity = self
                    .store
                    .identity_for_session(player.session_id)
                    .await?
                    .unwrap_or(player.session_id);
                self.record_integrity_signal(
                    &UserSession {
                        id: player.session_id,
                        account_id: (identity != player.session_id).then_some(identity),
                        nickname: player.nickname.clone(),
                        token_hash: String::new(),
                        created_at: result.finished_at,
                        last_seen_at: result.finished_at,
                        current_room_id: Some(room.id),
                    },
                    Some(room.id),
                    IntegritySignalKind::IntentionalStalling,
                    if result.finish_reason == FinishReason::TurnTimeout {
                        4
                    } else {
                        3
                    },
                    0.92,
                    serde_json::json!({
                        "protocolVersion": crate::PROTOCOL_VERSION,
                        "gameId": room.game_id,
                        "playerId": player.id,
                        "totalTimeouts": timeouts,
                        "finishReason": result.finish_reason,
                        "totalTurns": result.total_turns,
                    }),
                )
                .await;
            }
        }
        if human_players.len() == 2
            && result.total_turns <= 5
            && result.finish_reason != FinishReason::FleetDestroyed
        {
            let first_identity = self
                .store
                .identity_for_session(human_players[0].session_id)
                .await?
                .unwrap_or(human_players[0].session_id);
            let second_identity = self
                .store
                .identity_for_session(human_players[1].session_id)
                .await?
                .unwrap_or(human_players[1].session_id);
            let count = self
                .store
                .suspicious_short_match_count(
                    first_identity,
                    second_identity,
                    Utc::now() - chrono::Duration::days(7),
                )
                .await?;
            if count >= 3 {
                let first_session_id = human_players[0].session_id;
                for player in &human_players {
                    let identity = if player.session_id == first_session_id {
                        first_identity
                    } else {
                        second_identity
                    };
                    self.record_integrity_signal(
                        &UserSession {
                            id: player.session_id,
                            account_id: (identity != player.session_id).then_some(identity),
                            nickname: player.nickname.clone(),
                            token_hash: String::new(),
                            created_at: result.finished_at,
                            last_seen_at: result.finished_at,
                            current_room_id: Some(room.id),
                        },
                        Some(room.id),
                        IntegritySignalKind::Collusion,
                        4,
                        0.82,
                        serde_json::json!({
                            "protocolVersion": crate::PROTOCOL_VERSION,
                            "gameId": room.game_id,
                            "pairedIdentityIds": [first_identity, second_identity],
                            "suspiciousShortMatchesSevenDays": count,
                            "finishReason": result.finish_reason,
                            "totalTurns": result.total_turns,
                        }),
                    )
                    .await;
                }
            }
        }
        Ok(())
    }

    pub async fn moderate_player_report(
        &self,
        operator_id: String,
        report_id: Uuid,
        action: ModerationActionKind,
        reason: String,
        duration_hours: Option<u32>,
        reverses_action_id: Option<Uuid>,
    ) -> Result<ModerationAction, GameError> {
        let operator_id = operator_id.trim().to_string();
        if operator_id.len() < 2
            || operator_id.len() > 64
            || !operator_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'@' | b'-')
            })
        {
            return Err(GameError::InvalidRequest);
        }
        let reason = reason.trim().to_string();
        if reason.chars().count() < 4
            || reason.chars().count() > 1000
            || reason.chars().any(char::is_control)
        {
            return Err(GameError::InvalidRequest);
        }
        let expires_at = match action {
            ModerationActionKind::Suspend => {
                let hours = duration_hours.filter(|hours| (1..=8_760).contains(hours));
                let hours = hours.ok_or(GameError::InvalidRequest)?;
                if reverses_action_id.is_some() {
                    return Err(GameError::InvalidRequest);
                }
                Some(Utc::now() + chrono::Duration::hours(i64::from(hours)))
            }
            ModerationActionKind::Reverse => {
                if duration_hours.is_some() || reverses_action_id.is_none() {
                    return Err(GameError::InvalidRequest);
                }
                None
            }
            _ => {
                if duration_hours.is_some() || reverses_action_id.is_some() {
                    return Err(GameError::InvalidRequest);
                }
                None
            }
        };
        let stored = self
            .store
            .apply_moderation_action(&NewModerationAction {
                id: Uuid::new_v4(),
                report_id,
                operator_id,
                action,
                reason,
                expires_at,
                reverses_action_id,
                created_at: Utc::now(),
            })
            .await?;
        if matches!(
            action,
            ModerationActionKind::Suspend | ModerationActionKind::Ban
        ) {
            for session_id in self
                .store
                .session_ids_for_identity(stored.target_identity_id)
                .await?
            {
                self.close_session_everywhere(
                    session_id,
                    if action == ModerationActionKind::Ban {
                        GameError::AccountBanned
                    } else {
                        GameError::AccountSuspended
                    },
                )
                .await;
                self.disconnect_session(session_id).await;
            }
        }
        Ok(stored)
    }

    pub async fn revoke_account_session(
        &self,
        session: &UserSession,
        target_session_id: Uuid,
    ) -> Result<bool, GameError> {
        let account_id = session.account_id.ok_or(GameError::Unauthorized)?;
        if target_session_id == session.id {
            return Err(GameError::InvalidRequest);
        }
        self.close_session_everywhere(target_session_id, GameError::Unauthorized)
            .await;
        self.disconnect_session(target_session_id).await;
        self.store
            .delete_account_session(account_id, target_session_id)
            .await
    }

    pub async fn authenticate(
        &self,
        jar: &CookieJar,
        authorization: Option<&str>,
    ) -> Result<UserSession, GameError> {
        let token = authorization
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(ToOwned::to_owned)
            .or_else(|| {
                jar.get("mk01_session")
                    .map(|cookie| cookie.value().to_string())
            })
            .ok_or(GameError::Unauthorized)?;
        let session = self
            .store
            .session_by_token_hash(&hash_token(&token))
            .await?
            .ok_or(GameError::Unauthorized)?;
        let age = Utc::now().signed_duration_since(session.last_seen_at);
        if age.num_seconds() > self.settings.session_ttl.as_secs() as i64 {
            return Err(GameError::Unauthorized);
        }
        match self
            .store
            .active_penalty(session.account_id.unwrap_or(session.id), session.id)
            .await?
        {
            Some(ActivePenalty::Banned) => return Err(GameError::AccountBanned),
            Some(ActivePenalty::Suspended(_)) => return Err(GameError::AccountSuspended),
            None => {}
        }
        Ok(session)
    }
}
