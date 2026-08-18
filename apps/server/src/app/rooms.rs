use super::*;

impl AppState {
    pub async fn room(&self, id: Uuid) -> Result<Arc<Mutex<GameRoom>>, GameError> {
        if self.store.kind() == "postgres+redis" {
            let mut latest = self
                .store
                .room_by_id_authoritative(id)
                .await?
                .ok_or(GameError::RoomNotFound)?;
            if let Some(room) = self.rooms.get(&id).map(|entry| entry.clone()) {
                let mut cached = room.lock().await;
                if latest.persistence_revision > cached.persistence_revision {
                    self.reconcile_runtime_state(&mut latest).await?;
                    *cached = latest;
                }
                drop(cached);
                return Ok(room);
            }
            self.reconcile_runtime_state(&mut latest).await?;
            let deadlines: Vec<_> = latest
                .disconnected_deadlines
                .iter()
                .map(|(player_id, deadline)| (*player_id, *deadline))
                .collect();
            let turn_timer = latest.timer_state(Utc::now());
            let room = Arc::new(Mutex::new(latest));
            self.rooms.insert(id, room.clone());
            for (player_id, deadline) in deadlines {
                self.schedule_disconnect_expiry(id, player_id, deadline);
            }
            self.schedule_turn_expiry(turn_timer);
            self.schedule_ai_turn(id);
            return Ok(room);
        }
        if let Some(room) = self.rooms.get(&id) {
            return Ok(room.clone());
        }
        let mut room = self
            .store
            .room_by_id(id)
            .await?
            .ok_or(GameError::RoomNotFound)?;
        self.reconcile_runtime_state(&mut room).await?;
        let deadlines: Vec<_> = room
            .disconnected_deadlines
            .iter()
            .map(|(player_id, deadline)| (*player_id, *deadline))
            .collect();
        let turn_timer = room.timer_state(Utc::now());
        let room = Arc::new(Mutex::new(room));
        self.rooms.insert(id, room.clone());
        for (player_id, deadline) in deadlines {
            self.schedule_disconnect_expiry(id, player_id, deadline);
        }
        self.schedule_turn_expiry(turn_timer);
        self.schedule_ai_turn(id);
        Ok(room)
    }

    pub async fn room_by_code(&self, code: &str) -> Result<Arc<Mutex<GameRoom>>, GameError> {
        let normalized = code.trim().to_ascii_uppercase();
        let cached_rooms: Vec<_> = self
            .rooms
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        for room in cached_rooms {
            let room_id = {
                let cached = room.lock().await;
                (cached.code == normalized).then_some(cached.id)
            };
            if let Some(room_id) = room_id {
                return self.room(room_id).await;
            }
        }
        let mut room = self
            .store
            .room_by_code(&normalized)
            .await?
            .ok_or(GameError::RoomNotFound)?;
        self.reconcile_runtime_state(&mut room).await?;
        let id = room.id;
        let turn_timer = room.timer_state(Utc::now());
        let room = Arc::new(Mutex::new(room));
        self.rooms.insert(id, room.clone());
        self.schedule_turn_expiry(turn_timer);
        self.schedule_ai_turn(id);
        Ok(room)
    }

    pub async fn create_room(
        &self,
        session: &UserSession,
        input: CreateRoomInput,
    ) -> Result<GameRoom, GameError> {
        if session.current_room_id.is_some() {
            return Err(GameError::AlreadyJoined);
        }
        if self.store.active_rooms().await?.len() >= self.settings.max_active_rooms {
            return Err(GameError::CapacityReached);
        }
        let code = self.unique_room_code().await?;
        let mut room = GameRoom::new_with_rules(
            code,
            input.name.trim().to_string(),
            input.visibility,
            session,
            input.rules.unwrap_or_default(),
        )?;
        self.save_room(&mut room).await?;
        self.store
            .update_session_room(session.id, Some(room.id))
            .await?;
        self.rooms
            .insert(room.id, Arc::new(Mutex::new(room.clone())));
        Ok(room)
    }

    pub async fn create_practice_room(
        &self,
        session: &UserSession,
        difficulty: AiDifficulty,
    ) -> Result<GameRoom, GameError> {
        if session.current_room_id.is_some() {
            return Err(GameError::AlreadyJoined);
        }
        let ai_name = match difficulty {
            AiDifficulty::Recruit => "MK-AI RECRUIT",
            AiDifficulty::Officer => "MK-AI OFFICER",
            AiDifficulty::Admiral => "MK-AI ADMIRAL",
        };
        let (ai_session, _) = self.create_session(ai_name.to_string()).await?;
        let room = self
            .create_room(
                session,
                CreateRoomInput {
                    name: "AI 전술 훈련".to_string(),
                    visibility: RoomVisibility::Private,
                    rules: None,
                },
            )
            .await?;
        let room = self.join_room(&ai_session, &room.code).await?;
        let room_ref = self.room(room.id).await?;
        let mut room = room_ref.lock().await;
        room.configure_practice(session.id, ai_session.id, difficulty, practice_fleet())?;
        self.save_room(&mut room).await?;
        Ok(room.clone())
    }

    pub async fn join_room(
        &self,
        session: &UserSession,
        code: &str,
    ) -> Result<GameRoom, GameError> {
        if let Some(current) = session.current_room_id {
            let current_room = self.room(current).await?;
            let room = current_room.lock().await;
            if room
                .players
                .iter()
                .any(|player| player.session_id == session.id)
            {
                return Ok(room.clone());
            }
            return Err(GameError::AlreadyJoined);
        }
        let room = self.room_by_code(code).await?;
        let existing_session_ids: Vec<_> = {
            let room = room.lock().await;
            room.players
                .iter()
                .map(|player| player.session_id)
                .collect()
        };
        for existing_session_id in existing_session_ids {
            if self
                .sessions_blocked(session.id, existing_session_id)
                .await?
            {
                return Err(GameError::PlayerBlocked);
            }
        }
        let mut room = room.lock().await;
        room.join(session)?;
        self.save_room(&mut room).await?;
        self.store
            .update_session_room(session.id, Some(room.id))
            .await?;
        Ok(room.clone())
    }

    pub async fn leave_room(
        &self,
        session: &UserSession,
        room_id: Uuid,
    ) -> Result<GameRoom, GameError> {
        let room = self.room(room_id).await?;
        let mut room = room.lock().await;
        room.leave(session.id)?;
        self.save_room(&mut room).await?;
        self.store.update_session_room(session.id, None).await?;
        if room.game.as_ref().is_some_and(|game| game.result.is_some()) {
            self.cancel_turn_expiry(room.id);
        }
        Ok(room.clone())
    }

    pub async fn save_room(&self, room: &mut GameRoom) -> Result<(), GameError> {
        let lease = if self.store.kind() == "postgres+redis" && room.persistence_revision > 0 {
            match self
                .store
                .acquire_room_authority(room.id, self.instance_id, ROOM_AUTHORITY_LEASE_DURATION)
                .await?
            {
                Some(lease) => {
                    self.metrics
                        .room_authority_acquisitions
                        .fetch_add(1, Ordering::Relaxed);
                    Some(lease)
                }
                None => {
                    self.metrics
                        .room_authority_conflicts
                        .fetch_add(1, Ordering::Relaxed);
                    if let Ok(Some(latest)) = self.store.room_by_id_authoritative(room.id).await {
                        *room = latest;
                    } else {
                        self.rooms.remove(&room.id);
                    }
                    return Err(GameError::VersionConflict);
                }
            }
        } else {
            None
        };
        let save_result = if let Some(lease) = lease {
            self.store.save_room_fenced(room, lease).await
        } else {
            self.store.save_room(room).await
        };
        if save_result.is_err() {
            if let Some(lease) = lease {
                let _ = self.store.release_room_authority(lease).await;
            }
        }
        match save_result {
            Ok(()) => {
                self.metrics.room_mutations.fetch_add(1, Ordering::Relaxed);
                if room.game.as_ref().is_some_and(|game| game.result.is_some()) {
                    if let Err(error) = self.detect_finished_match_integrity(room).await {
                        tracing::error!(
                            room_id = %room.id,
                            error_code = error.code(),
                            "finished match integrity assessment failed"
                        );
                    }
                }
                Ok(())
            }
            Err(error) => {
                if error == GameError::VersionConflict {
                    self.metrics
                        .room_version_conflicts
                        .fetch_add(1, Ordering::Relaxed);
                }
                if let Ok(Some(latest)) = self.store.room_by_id_authoritative(room.id).await {
                    *room = latest;
                } else {
                    self.rooms.remove(&room.id);
                }
                Err(error)
            }
        }
    }

    pub(super) async fn reconcile_runtime_state(
        &self,
        room: &mut GameRoom,
    ) -> Result<(), GameError> {
        for _ in 0..3 {
            if !room.ensure_runtime_state(self.settings.turn_duration_seconds, Utc::now()) {
                return Ok(());
            }
            match self.save_room(room).await {
                Ok(()) => return Ok(()),
                Err(GameError::VersionConflict) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(GameError::VersionConflict)
    }

    pub async fn revoke_session(
        &self,
        session: &UserSession,
    ) -> Result<Option<GameRoom>, GameError> {
        let _ = self.cancel_matchmaking(session.id).await;
        let departed_room = if let Some(room_id) = session.current_room_id {
            match self.leave_room(session, room_id).await {
                Ok(room) => Some(room),
                Err(error) => {
                    tracing::warn!(
                        session_id = %session.id,
                        room_id = %room_id,
                        error_code = error.code(),
                        "session revoked after room departure failed"
                    );
                    None
                }
            }
        } else {
            None
        };
        self.hub.close(session.id);
        self.store.delete_session(session.id).await?;
        Ok(departed_room)
    }

    pub fn invite_url(&self, code: &str) -> String {
        format!(
            "{}/join/{}",
            self.settings.public_base_url.trim_end_matches('/'),
            code
        )
    }

    pub async fn broadcast_snapshots(&self, room: &GameRoom, kind: SnapshotEvent) {
        for player in &room.players {
            if let Ok(snapshot) = room.snapshot_for(player.session_id) {
                let event = match kind {
                    SnapshotEvent::RoomUpdated => ServerEvent::RoomUpdated(snapshot),
                    SnapshotEvent::PlayerJoined => ServerEvent::PlayerJoined(snapshot),
                    SnapshotEvent::PlayerLeft => ServerEvent::PlayerLeft(snapshot),
                    SnapshotEvent::GamePlacementStarted => {
                        ServerEvent::GamePlacementStarted(snapshot)
                    }
                    SnapshotEvent::PlacementAccepted => ServerEvent::PlacementAccepted(snapshot),
                    SnapshotEvent::GameStarted => ServerEvent::GameStarted(snapshot),
                    SnapshotEvent::TurnChanged => ServerEvent::TurnChanged(snapshot),
                    SnapshotEvent::GameFinished => ServerEvent::GameFinished(snapshot),
                    SnapshotEvent::PlayerDisconnected => ServerEvent::PlayerDisconnected(snapshot),
                    SnapshotEvent::PlayerReconnected => ServerEvent::PlayerReconnected(snapshot),
                    SnapshotEvent::GameSnapshot => ServerEvent::GameSnapshot(snapshot),
                };
                self.send_to_session(player.session_id, event).await;
            }
        }
    }

    pub async fn broadcast_chat_message(&self, room: &GameRoom, message: &ChatMessage) {
        for player in &room.players {
            if let Some(sender_player_id) = message.player_id {
                let Some(sender) = room
                    .players
                    .iter()
                    .find(|candidate| candidate.id == sender_player_id)
                else {
                    continue;
                };
                match self
                    .communication_suppressed(player.session_id, sender.session_id)
                    .await
                {
                    Ok(true) => continue,
                    Ok(false) => {}
                    Err(error) => {
                        tracing::error!(
                            error_code = error.code(),
                            recipient_session_id = %player.session_id,
                            "chat relationship check failed closed"
                        );
                        continue;
                    }
                }
            }
            self.send_to_session(player.session_id, ServerEvent::ChatMessage(message.clone()))
                .await;
        }
    }

    pub async fn broadcast_latest_chat_message(&self, room: &GameRoom) {
        if let Some(message) = room.chat_messages.last() {
            self.broadcast_chat_message(room, message).await;
        }
    }

    pub async fn broadcast_chat_typing(&self, room: &GameRoom, event: &ChatTypingEvent) {
        let Some(sender) = room
            .players
            .iter()
            .find(|candidate| candidate.id == event.player_id)
        else {
            return;
        };
        for player in &room.players {
            if player.id != event.player_id
                && self
                    .communication_suppressed(player.session_id, sender.session_id)
                    .await
                    .is_ok_and(|suppressed| !suppressed)
            {
                self.send_to_session(player.session_id, ServerEvent::ChatTyping(event.clone()))
                    .await;
            }
        }
    }

    pub async fn chat_history_for(
        &self,
        room: &GameRoom,
        recipient_session_id: Uuid,
    ) -> Result<Vec<ChatMessage>, GameError> {
        let messages = room.chat_history(recipient_session_id)?;
        let mut filtered = Vec::with_capacity(messages.len());
        for message in messages {
            let Some(sender_player_id) = message.player_id else {
                filtered.push(message);
                continue;
            };
            let Some(sender) = room
                .players
                .iter()
                .find(|player| player.id == sender_player_id)
            else {
                continue;
            };
            if !self
                .communication_suppressed(recipient_session_id, sender.session_id)
                .await?
            {
                filtered.push(message);
            }
        }
        Ok(filtered)
    }

    async fn communication_suppressed(
        &self,
        recipient_session_id: Uuid,
        sender_session_id: Uuid,
    ) -> Result<bool, GameError> {
        if recipient_session_id == sender_session_id {
            return Ok(false);
        }
        let Some(recipient_identity) = self
            .store
            .identity_for_session(recipient_session_id)
            .await?
        else {
            return Ok(true);
        };
        let Some(sender_identity) = self.store.identity_for_session(sender_session_id).await?
        else {
            return Ok(true);
        };
        Ok(self
            .store
            .social_relationship_between(recipient_identity, sender_identity)
            .await?
            .is_some_and(|relationship| relationship.muted || relationship.blocked))
    }

    async fn sessions_blocked(
        &self,
        first_session_id: Uuid,
        second_session_id: Uuid,
    ) -> Result<bool, GameError> {
        let first_identity = self
            .store
            .identity_for_session(first_session_id)
            .await?
            .ok_or(GameError::Unauthorized)?;
        let second_identity = self
            .store
            .identity_for_session(second_session_id)
            .await?
            .ok_or(GameError::Unauthorized)?;
        let first_blocks = self
            .store
            .social_relationship_between(first_identity, second_identity)
            .await?
            .is_some_and(|relationship| relationship.blocked);
        let second_blocks = self
            .store
            .social_relationship_between(second_identity, first_identity)
            .await?
            .is_some_and(|relationship| relationship.blocked);
        Ok(first_blocks || second_blocks)
    }
}

pub(super) fn practice_fleet() -> Vec<ShipPlacement> {
    ShipKind::ALL
        .into_iter()
        .enumerate()
        .map(|(index, kind)| ShipPlacement {
            kind,
            origin: Coordinate {
                row: (index as u8) * 2,
                col: 0,
            },
            orientation: Orientation::Horizontal,
        })
        .collect()
}
