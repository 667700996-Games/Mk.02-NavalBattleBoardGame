use super::*;

impl AppState {
    pub async fn enqueue_matchmaking(
        &self,
        session: UserSession,
        preferences: MatchmakingPreferences,
    ) -> Result<MatchmakingOutcome, GameError> {
        if session.current_room_id.is_some() {
            return Err(GameError::AlreadyJoined);
        }
        let preferences = preferences.validate()?;
        let (criteria, ranked_context) = match preferences.pool {
            MatchmakingPool::Casual => (MatchmakingCriteria::casual(session.id), None),
            MatchmakingPool::Ranked => {
                let account_id = session.account_id.ok_or(GameError::RankedAccountRequired)?;
                let now = Utc::now();
                let content = self.active_live_content(now).await?;
                if now < content.season.starts_at || now >= content.season.ends_at {
                    return Err(GameError::RankedSeasonUnavailable);
                }
                let profile = self
                    .store
                    .ranked_profile(
                        account_id,
                        &content.season.id,
                        content.season.starts_at,
                        now,
                    )
                    .await?;
                (
                    MatchmakingCriteria::ranked(
                        account_id,
                        preferences.region,
                        preferences.latency_ms.ok_or(GameError::InvalidRequest)?,
                        profile.rating,
                        ranked_season_key(&content.season.id),
                    )?,
                    Some(RankedMatchContext {
                        season_id: content.season.id,
                        content_revision: content.revision,
                    }),
                )
            }
        };
        if self.store.matchmaking_entry(session.id).await?.is_none()
            && self.store.matchmaking_queue_stats().await?.queued
                >= self.settings.max_matchmaking_queue
        {
            return Err(GameError::CapacityReached);
        }
        let queued = self.store.enqueue_matchmaking(&session, criteria).await?;
        let own_queued_at = queued.queued_at;
        let Some(claim) = queued.claim else {
            self.metrics
                .matchmaking_queued
                .fetch_add(1, Ordering::Relaxed);
            if criteria.pool == MatchmakingPool::Ranked {
                self.metrics
                    .ranked_matchmaking_queued
                    .fetch_add(1, Ordering::Relaxed);
            }
            return Ok(MatchmakingOutcome {
                room: None,
                queued_at: Some(own_queued_at),
                criteria: queued.criteria,
                search_window: MatchmakingSearchWindow::at(own_queued_at, Utc::now()),
                quality: None,
            });
        };
        let claim_id = claim.id;
        let opponent_queued_at = claim.opponent_queued_at;
        let quality = claim.quality;
        let result = async {
            let code = self.unique_room_code().await?;
            let mut room = GameRoom::new(
                code,
                if criteria.pool == MatchmakingPool::Ranked {
                    "랭크 교전".to_string()
                } else {
                    "신속 교전".to_string()
                },
                RoomVisibility::Private,
                &claim.opponent,
            )?;
            room.join(&session)?;
            room.matchmaking_quality = Some(quality);
            room.ranked_match = ranked_context;
            self.store.complete_matchmaking(claim_id, &mut room).await?;
            self.metrics.record_matchmaking_latency(own_queued_at);
            self.metrics.record_matchmaking_latency(opponent_queued_at);
            self.metrics
                .matchmaking_completed
                .fetch_add(1, Ordering::Relaxed);
            if criteria.pool == MatchmakingPool::Ranked {
                self.metrics
                    .ranked_matchmaking_completed
                    .fetch_add(1, Ordering::Relaxed);
                if quality.rematch_relaxed {
                    self.metrics
                        .ranked_matchmaking_rematches
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
            self.rooms
                .insert(room.id, Arc::new(Mutex::new(room.clone())));
            self.broadcast_snapshots(&room, SnapshotEvent::PlayerJoined)
                .await;
            self.broadcast_latest_chat_message(&room).await;
            Ok::<_, GameError>(MatchmakingOutcome {
                room: Some(room),
                queued_at: None,
                criteria: queued.criteria,
                search_window: MatchmakingSearchWindow::at(own_queued_at, Utc::now()),
                quality: Some(quality),
            })
        }
        .await;
        if result.is_err() {
            if let Err(release_error) = self.store.release_matchmaking_claim(claim_id).await {
                tracing::error!(
                    %claim_id,
                    error_code = release_error.code(),
                    "matchmaking claim release failed"
                );
            }
        }
        result
    }

    pub async fn cancel_matchmaking(&self, session_id: Uuid) -> Result<bool, GameError> {
        let cancelled = self.store.cancel_matchmaking(session_id).await?;
        if cancelled {
            self.metrics
                .matchmaking_cancelled
                .fetch_add(1, Ordering::Relaxed);
        }
        Ok(cancelled)
    }

    pub async fn matchmaking_entry(
        &self,
        session_id: Uuid,
    ) -> Result<Option<MatchmakingQueueEntry>, GameError> {
        self.store.matchmaking_entry(session_id).await
    }

    pub(super) async fn unique_room_code(&self) -> Result<String, GameError> {
        const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        for _ in 0..20 {
            let code: String = (0..6)
                .map(|_| {
                    let index = rand::random_range(0..ALPHABET.len());
                    ALPHABET[index] as char
                })
                .collect();
            if self.store.room_by_code(&code).await?.is_none() {
                return Ok(code);
            }
        }
        Err(GameError::Internal)
    }
}
