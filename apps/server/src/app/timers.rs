use super::*;

impl AppState {
    pub async fn broadcast_timer_state(
        &self,
        room: &GameRoom,
        event: fn(GameTimerState) -> ServerEvent,
    ) {
        if let Some(timer) = room.timer_state(Utc::now()) {
            for player in &room.players {
                self.send_to_session(player.session_id, event(timer.clone()))
                    .await;
            }
        }
    }

    pub async fn restore_connection(&self, session: &UserSession) {
        let Some(room_id) = session.current_room_id else {
            return;
        };
        let Ok(room) = self.room(room_id).await else {
            return;
        };
        let mut room = room.lock().await;
        let disconnected_at = room
            .player_for_session(session.id)
            .ok()
            .and_then(|player| room.disconnected_deadlines.get(&player.id).copied())
            .and_then(|deadline| {
                chrono::Duration::from_std(self.settings.reconnect_grace)
                    .ok()
                    .map(|grace| deadline - grace)
            });
        if matches!(room.reconnect(session.id), Ok(true)) {
            if self.save_room(&mut room).await.is_err() {
                return;
            }
            if let Some(disconnected_at) = disconnected_at {
                self.metrics.record_active_match_recovery(
                    Utc::now()
                        .signed_duration_since(disconnected_at)
                        .to_std()
                        .unwrap_or_default(),
                );
            }
            self.broadcast_latest_chat_message(&room).await;
            self.broadcast_snapshots(&room, SnapshotEvent::PlayerReconnected)
                .await;
        }
    }

    pub async fn disconnect_session(&self, session_id: Uuid) {
        let room_refs: Vec<_> = self
            .rooms
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        for room_ref in room_refs {
            let mut room = room_ref.lock().await;
            if !room
                .players
                .iter()
                .any(|player| player.session_id == session_id)
            {
                continue;
            }
            let grace = self.settings.reconnect_grace.as_secs() as i64;
            let Ok(deadline) = room.disconnect(session_id, grace) else {
                continue;
            };
            let room_id = room.id;
            let player_id = match room.player_for_session(session_id) {
                Ok(player) => player.id,
                Err(_) => continue,
            };
            if self.save_room(&mut room).await.is_err() {
                return;
            }
            self.broadcast_latest_chat_message(&room).await;
            self.broadcast_snapshots(&room, SnapshotEvent::PlayerDisconnected)
                .await;
            drop(room);

            self.schedule_disconnect_expiry(room_id, player_id, deadline);
            break;
        }
    }

    pub(super) async fn restore_active_rooms(&self) -> Result<(), GameError> {
        for mut room in self.store.active_rooms().await? {
            let room_id = room.id;
            self.reconcile_runtime_state(&mut room).await?;
            let deadlines: Vec<_> = room
                .disconnected_deadlines
                .iter()
                .map(|(player_id, deadline)| (*player_id, *deadline))
                .collect();
            let turn_timer = room.timer_state(Utc::now());
            self.rooms.insert(room_id, Arc::new(Mutex::new(room)));
            for (player_id, deadline) in deadlines {
                self.schedule_disconnect_expiry(room_id, player_id, deadline);
            }
            self.schedule_turn_expiry(turn_timer);
            self.schedule_ai_turn(room_id);
        }
        Ok(())
    }

    pub(super) fn schedule_disconnect_expiry(
        &self,
        room_id: Uuid,
        player_id: Uuid,
        deadline: chrono::DateTime<Utc>,
    ) {
        let state = self.clone();
        tokio::spawn(async move {
            let delay = (deadline - Utc::now()).to_std().unwrap_or_default();
            tokio::time::sleep(delay).await;
            let Ok(room_ref) = state.room(room_id).await else {
                return;
            };
            let mut room = room_ref.lock().await;
            let expired_session_id = room
                .players
                .iter()
                .find(|player| player.id == player_id)
                .map(|player| player.session_id);
            if room
                .expire_disconnect(player_id, Utc::now())
                .unwrap_or(false)
            {
                if state.save_room(&mut room).await.is_err() {
                    return;
                }
                if room.status != crate::domain::RoomStatus::Playing {
                    state.cancel_turn_expiry(room.id);
                }
                if let Some(session_id) = expired_session_id {
                    let _ = state.store.update_session_room(session_id, None).await;
                }
                state.broadcast_latest_chat_message(&room).await;
                state
                    .broadcast_snapshots(
                        &room,
                        if room.status == crate::domain::RoomStatus::Finished {
                            SnapshotEvent::GameFinished
                        } else {
                            SnapshotEvent::PlayerLeft
                        },
                    )
                    .await;
            }
        });
    }

    pub fn schedule_turn_expiry(&self, timer: Option<GameTimerState>) {
        let Some(timer) = timer else {
            return;
        };
        let Some(deadline) = timer.turn_deadline_at else {
            self.cancel_turn_expiry(timer.room_id);
            return;
        };
        let key = TurnTimerKey {
            turn_number: timer.turn_number,
            active_player_id: timer.active_player_id,
            deadline,
        };
        if self
            .turn_timers
            .get(&timer.room_id)
            .is_some_and(|current| *current == key)
        {
            return;
        }
        self.turn_timers.insert(timer.room_id, key.clone());
        let state = self.clone();
        tokio::spawn(async move {
            // Tokio can resume a timer a few scheduling ticks before the wall-clock deadline.
            // Keep the server-side timer armed until the authoritative UTC deadline has passed;
            // otherwise `Game::expire_turn` would correctly reject the early expiry and leave
            // an inactive turn with no replacement timer.
            loop {
                let remaining = deadline - Utc::now();
                if remaining <= chrono::Duration::zero() {
                    break;
                }
                tokio::time::sleep(remaining.to_std().unwrap_or_default()).await;
            }
            let still_current = state
                .turn_timers
                .get(&timer.room_id)
                .is_some_and(|current| *current == key);
            if !still_current {
                return;
            }
            let Ok(room_ref) = state.room(timer.room_id).await else {
                state.cancel_turn_expiry(timer.room_id);
                return;
            };
            let mut room = room_ref.lock().await;
            if !state.resolve_turn_expiry(&mut room, &key).await
                && state
                    .turn_timers
                    .get(&timer.room_id)
                    .is_some_and(|current| *current == key)
            {
                state.cancel_turn_expiry(timer.room_id);
            }
        });
    }

    async fn resolve_turn_expiry(&self, room: &mut GameRoom, key: &TurnTimerKey) -> bool {
        let record = room
            .expire_turn(
                key.turn_number,
                key.active_player_id,
                key.deadline,
                Utc::now(),
            )
            .unwrap_or(None);
        let Some(record) = record else {
            return false;
        };
        let finished = record.winner_id.is_some();
        let next_timer = room.timer_state(Utc::now());
        if self.save_room(room).await.is_err() {
            return false;
        }
        for player in &room.players {
            self.send_to_session(player.session_id, ServerEvent::TurnExpired(record.clone()))
                .await;
        }
        self.broadcast_latest_chat_message(room).await;
        self.broadcast_snapshots(
            room,
            if finished {
                SnapshotEvent::GameFinished
            } else {
                SnapshotEvent::TurnChanged
            },
        )
        .await;
        if finished {
            self.cancel_turn_expiry(room.id);
        } else {
            self.broadcast_timer_state(room, ServerEvent::TurnStarted)
                .await;
            self.schedule_turn_expiry(next_timer);
            self.schedule_ai_turn(room.id);
        }
        true
    }

    pub fn schedule_ai_turn(&self, room_id: Uuid) {
        let state = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(650)).await;
            let Ok(room_ref) = state.room(room_id).await else {
                return;
            };
            let mut room = room_ref.lock().await;
            let Some(game) = room.game.as_ref() else {
                return;
            };
            if room.status != crate::domain::RoomStatus::Playing || game.result.is_some() {
                return;
            }
            let Some(ai_player) = room
                .players
                .iter()
                .find(|player| player.kind == PlayerKind::Ai && player.id == game.current_player_id)
                .cloned()
            else {
                return;
            };
            let Some(coordinate) = select_ai_coordinate(&room, ai_player.id) else {
                return;
            };
            let expected_version = room.version;
            let expected_turn = game.turn_number;
            let Ok((record, _)) = room.fire(
                ai_player.session_id,
                Uuid::new_v4(),
                ai_player.id,
                coordinate,
                expected_version,
                expected_turn,
            ) else {
                return;
            };
            if state.save_room(&mut room).await.is_err() {
                return;
            }
            let next_timer = room.timer_state(Utc::now());
            for player in &room.players {
                state
                    .send_to_session(player.session_id, ServerEvent::AttackResult(record.clone()))
                    .await;
                if record.sunk_ship.is_some() {
                    state
                        .send_to_session(player.session_id, ServerEvent::ShipSunk(record.clone()))
                        .await;
                }
            }
            if record.winner_id.is_some() {
                state.broadcast_latest_chat_message(&room).await;
            }
            state
                .broadcast_snapshots(
                    &room,
                    if record.winner_id.is_some() {
                        SnapshotEvent::GameFinished
                    } else {
                        SnapshotEvent::TurnChanged
                    },
                )
                .await;
            if record.winner_id.is_some() {
                state.cancel_turn_expiry(room.id);
            } else {
                state
                    .broadcast_timer_state(&room, ServerEvent::TurnStarted)
                    .await;
                state.schedule_turn_expiry(next_timer);
            }
        });
    }

    pub fn cancel_turn_expiry(&self, room_id: Uuid) {
        self.turn_timers.remove(&room_id);
    }
}

pub(super) fn select_ai_coordinate(room: &GameRoom, ai_player_id: Uuid) -> Option<Coordinate> {
    let game = room.game.as_ref()?;
    let used: HashSet<_> = game
        .attacks
        .iter()
        .filter(|attack| attack.attacker_id == ai_player_id)
        .map(|attack| attack.coordinate)
        .collect();
    let difficulty = room.practice_difficulty.unwrap_or_default();
    if difficulty != AiDifficulty::Recruit {
        for attack in game.attacks.iter().rev().filter(|attack| {
            attack.attacker_id == ai_player_id && attack.outcome == AttackOutcome::Hit
        }) {
            let row = i16::from(attack.coordinate.row);
            let col = i16::from(attack.coordinate.col);
            for (row_offset, col_offset) in [(-1_i16, 0_i16), (0, 1), (1, 0), (0, -1)] {
                let next_row = row + row_offset;
                let next_col = col + col_offset;
                if (0..10).contains(&next_row) && (0..10).contains(&next_col) {
                    let coordinate = Coordinate {
                        row: next_row as u8,
                        col: next_col as u8,
                    };
                    if !used.contains(&coordinate) {
                        return Some(coordinate);
                    }
                }
            }
        }
    }

    let mut candidates: Vec<_> = (0_u8..10)
        .flat_map(|row| (0_u8..10).map(move |col| Coordinate { row, col }))
        .filter(|coordinate| !used.contains(coordinate))
        .collect();
    if difficulty == AiDifficulty::Admiral {
        let parity: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|coordinate| (coordinate.row + coordinate.col) % 2 == 0)
            .collect();
        if !parity.is_empty() {
            candidates = parity;
        }
    }
    if candidates.is_empty() {
        return None;
    }
    let seed = room.id.as_u128() ^ u128::from(game.turn_number).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    Some(candidates[(seed as usize) % candidates.len()])
}
