use super::*;

impl GameRoom {
    pub fn ensure_runtime_state(&mut self, turn_duration_seconds: u32, now: DateTime<Utc>) -> bool {
        let mut changed = false;
        if self.host_player_id.is_nil() {
            if let Some(host) = self
                .players
                .iter()
                .find(|player| player.is_host)
                .or_else(|| self.players.first())
            {
                self.host_player_id = host.id;
                changed = true;
            }
        }
        for player in &mut self.players {
            let expected_role = if player.id == self.host_player_id {
                PlayerRole::Host
            } else {
                PlayerRole::Guest
            };
            if player.role != expected_role || player.is_host != (expected_role == PlayerRole::Host)
            {
                player.role = expected_role;
                player.is_host = expected_role == PlayerRole::Host;
                changed = true;
            }
            if player.ready_state == PlayerReadyState::NotReady && player.ready_at.take().is_some()
            {
                changed = true;
            }
        }
        if self.game_id.is_none() {
            if let Some(game) = &self.game {
                self.game_id = Some(Uuid::new_v4());
                self.placement_started_at.get_or_insert(game.started_at);
                self.status = if game.result.is_some() {
                    RoomStatus::Finished
                } else {
                    RoomStatus::Playing
                };
                changed = true;
            } else if self.status == RoomStatus::Placement {
                self.pending_placements.clear();
                for player in &mut self.players {
                    player.ready_state = PlayerReadyState::NotReady;
                    player.ready_at = None;
                    player.placement_confirmed = false;
                }
                self.refresh_lobby_status();
                changed = true;
            }
        }
        if self.game_id.is_none()
            && matches!(
                self.status,
                RoomStatus::WaitingForOpponent
                    | RoomStatus::WaitingForReady
                    | RoomStatus::ReadyToStart
            )
        {
            let previous = self.status;
            self.refresh_lobby_status();
            changed |= self.status != previous;
        }
        if self.is_active_battle()
            && self
                .game
                .as_mut()
                .is_some_and(|game| game.ensure_turn_timer(turn_duration_seconds, now))
        {
            changed = true;
        }
        if changed {
            self.updated_at = now;
        }
        changed
    }

    pub fn timer_state(&self, now: DateTime<Utc>) -> Option<GameTimerState> {
        let game = self.game.as_ref()?;
        if game.result.is_some() {
            return None;
        }
        Some(GameTimerState {
            room_id: self.id,
            game_id: self.game_id.unwrap_or(self.id),
            turn_number: game.turn_number,
            active_player_id: game.current_player_id,
            game_started_at: game.started_at,
            turn_started_at: game.turn_started_at,
            turn_deadline_at: game.turn_deadline_at,
            turn_duration_seconds: game.turn_duration_seconds,
            server_timestamp: now,
        })
    }

    pub fn expire_turn(
        &mut self,
        expected_turn: u32,
        expected_player_id: Uuid,
        expected_deadline: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Option<TurnExpiredRecord>, GameError> {
        self.require_executable_balance()?;
        if !self.is_active_battle() {
            return Ok(None);
        }
        let expiration = self
            .game
            .as_mut()
            .ok_or(GameError::InvalidState)?
            .expire_turn(expected_turn, expected_player_id, expected_deadline, now)?;
        let Some(expiration) = expiration else {
            return Ok(None);
        };
        if expiration.winner_id.is_some() {
            self.status = RoomStatus::Finished;
            self.disconnected_deadlines.clear();
        }
        self.bump();
        let nickname = self
            .players
            .iter()
            .find(|player| player.id == expiration.expired_player_id)
            .map(|player| player.nickname.as_str())
            .unwrap_or("상대");
        let message = if expiration.winner_id.is_some() {
            format!(
                "{} 지휘관이 {}회 연속 시간 초과로 자동 기권 처리되었습니다.",
                nickname, self.balance.manifest.consecutive_timeout_forfeit
            )
        } else {
            format!("{nickname} 지휘관의 작전 시간이 만료되었습니다. 공격 기회가 소멸했습니다.")
        };
        self.push_system_message(message);
        Ok(Some(TurnExpiredRecord::from_expiration(
            self.id,
            self.game_id.unwrap_or(self.id),
            expiration,
            now,
        )))
    }

    fn is_active_battle(&self) -> bool {
        self.status == RoomStatus::Playing
    }

    pub fn disconnect(
        &mut self,
        session_id: Uuid,
        grace_seconds: i64,
    ) -> Result<DateTime<Utc>, GameError> {
        if matches!(self.status, RoomStatus::Finished | RoomStatus::Cancelled) {
            return Err(GameError::InvalidState);
        }
        let disconnected_player = self.player_for_session(session_id)?.clone();
        let player_id = disconnected_player.id;
        if let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        {
            player.connection_state = ConnectionState::Reconnecting;
        }
        let deadline = Utc::now() + Duration::seconds(grace_seconds);
        self.disconnected_deadlines.insert(player_id, deadline);
        self.bump();
        self.push_system_message(format!(
            "{} 지휘관의 연결이 끊겼습니다. 재접속을 기다립니다.",
            disconnected_player.nickname
        ));
        Ok(deadline)
    }

    pub fn reconnect(&mut self, session_id: Uuid) -> Result<bool, GameError> {
        let reconnecting_player = self.player_for_session(session_id)?.clone();
        let player_id = reconnecting_player.id;
        let was_reconnecting = reconnecting_player.connection_state != ConnectionState::Online
            || self.disconnected_deadlines.contains_key(&player_id);
        if !was_reconnecting {
            return Ok(false);
        }
        if let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        {
            player.connection_state = ConnectionState::Online;
        }
        self.disconnected_deadlines.remove(&player_id);
        self.bump();
        self.push_system_message(format!(
            "{} 지휘관이 전투 채널에 재접속했습니다.",
            reconnecting_player.nickname
        ));
        Ok(true)
    }

    pub fn expire_disconnect(
        &mut self,
        player_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<bool, GameError> {
        let Some(deadline) = self.disconnected_deadlines.get(&player_id).copied() else {
            return Ok(false);
        };
        if deadline > now {
            return Ok(false);
        }
        let disconnected_player = self
            .players
            .iter()
            .find(|player| player.id == player_id)
            .cloned()
            .ok_or(GameError::NotRoomMember)?;
        let disconnected_nickname = disconnected_player.nickname.clone();
        if let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        {
            player.connection_state = ConnectionState::Offline;
        }
        let opponent_id = self
            .players
            .iter()
            .find(|player| player.id != player_id)
            .map(|player| player.id);
        let is_lobby = matches!(
            self.status,
            RoomStatus::WaitingForOpponent | RoomStatus::WaitingForReady | RoomStatus::ReadyToStart
        ) && self.game_id.is_none();
        if is_lobby && player_id != self.host_player_id {
            self.players.retain(|player| player.id != player_id);
            self.reset_lobby_after_guest_departure();
        } else if is_lobby {
            self.status = RoomStatus::Cancelled;
        } else if let (Some(game), Some(winner_id)) = (self.game.as_mut(), opponent_id) {
            game.forfeit(winner_id, FinishReason::DisconnectTimeout)?;
            self.status = RoomStatus::Finished;
        } else {
            self.status = RoomStatus::Cancelled;
        }
        self.disconnected_deadlines.remove(&player_id);
        self.bump();
        let message = if is_lobby && player_id != self.host_player_id {
            format!(
                "{disconnected_nickname} 지휘관의 재접속 시간이 만료되어 자리에서 제거되었습니다."
            )
        } else if is_lobby {
            "방장의 재접속 시간이 만료되어 작전실이 종료되었습니다.".to_string()
        } else if let Some(winner_id) = opponent_id {
            let winner = self
                .players
                .iter()
                .find(|player| player.id == winner_id)
                .map(|player| player.nickname.as_str())
                .unwrap_or("상대");
            format!(
                "{disconnected_nickname} 지휘관의 재접속 시간이 만료되었습니다. {winner} 지휘관이 승리했습니다."
            )
        } else {
            format!(
                "{disconnected_nickname} 지휘관의 재접속 시간이 만료되어 작전이 취소되었습니다."
            )
        };
        self.push_system_message(message);
        Ok(true)
    }
}
