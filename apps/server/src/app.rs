use std::{collections::VecDeque, sync::Arc, time::Duration};

use axum::{
    Router,
    http::{HeaderValue, Method},
    routing::get,
};
use axum_extra::extract::CookieJar;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use dashmap::DashMap;
use rand::RngCore;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, mpsc};
use tower_http::{
    catch_panic::CatchPanicLayer, compression::CompressionLayer, cors::CorsLayer,
    timeout::TimeoutLayer, trace::TraceLayer,
};
use uuid::Uuid;

use crate::{
    api,
    config::{Settings, StorageMode},
    domain::{ChatMessage, ChatTypingEvent, GameRoom, GameTimerState, RoomVisibility, UserSession},
    error::GameError,
    protocol::{CreateRoomInput, ServerEvent},
    store::{GameStore, MemoryStore, PostgresRedisStore},
    ws,
};

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub store: Arc<dyn GameStore>,
    pub rooms: Arc<DashMap<Uuid, Arc<Mutex<GameRoom>>>>,
    pub hub: ConnectionHub,
    matchmaking: Arc<Mutex<VecDeque<QueuedSession>>>,
    turn_timers: Arc<DashMap<Uuid, TurnTimerKey>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppState")
            .field("storage", &self.store.kind())
            .field("cached_rooms", &self.rooms.len())
            .finish()
    }
}

#[derive(Debug, Clone)]
struct QueuedSession {
    session: UserSession,
    queued_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TurnTimerKey {
    turn_number: u32,
    active_player_id: Uuid,
    deadline: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub enum SnapshotEvent {
    RoomUpdated,
    PlayerJoined,
    PlayerLeft,
    GamePlacementStarted,
    PlacementAccepted,
    GameStarted,
    TurnChanged,
    GameFinished,
    PlayerDisconnected,
    PlayerReconnected,
    GameSnapshot,
}

impl AppState {
    pub async fn new(settings: Settings) -> Result<Self, GameError> {
        let store: Arc<dyn GameStore> = match settings.storage_mode {
            StorageMode::Memory => Arc::new(MemoryStore::default()),
            StorageMode::Postgres => Arc::new(
                PostgresRedisStore::connect(&settings.database_url, &settings.redis_url).await?,
            ),
        };
        let state = Self {
            settings: Arc::new(settings),
            store,
            rooms: Arc::new(DashMap::new()),
            hub: ConnectionHub::default(),
            matchmaking: Arc::new(Mutex::new(VecDeque::new())),
            turn_timers: Arc::new(DashMap::new()),
        };
        state.restore_active_rooms().await?;
        Ok(state)
    }

    pub fn with_store(settings: Settings, store: Arc<dyn GameStore>) -> Self {
        Self {
            settings: Arc::new(settings),
            store,
            rooms: Arc::new(DashMap::new()),
            hub: ConnectionHub::default(),
            matchmaking: Arc::new(Mutex::new(VecDeque::new())),
            turn_timers: Arc::new(DashMap::new()),
        }
    }

    pub async fn create_session(
        &self,
        nickname: String,
    ) -> Result<(UserSession, String), GameError> {
        let nickname = nickname.trim().to_string();
        validate_nickname(&nickname)?;
        let mut bytes = [0_u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let token = URL_SAFE_NO_PAD.encode(bytes);
        let token_hash = hash_token(&token);
        let now = Utc::now();
        let session = UserSession {
            id: Uuid::new_v4(),
            nickname,
            token_hash,
            created_at: now,
            last_seen_at: now,
            current_room_id: None,
        };
        self.store.save_session(&session).await?;
        Ok((session, token))
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
        Ok(session)
    }

    pub async fn room(&self, id: Uuid) -> Result<Arc<Mutex<GameRoom>>, GameError> {
        if let Some(room) = self.rooms.get(&id) {
            return Ok(room.clone());
        }
        let mut room = self
            .store
            .room_by_id(id)
            .await?
            .ok_or(GameError::RoomNotFound)?;
        let changed = room.ensure_runtime_state(self.settings.turn_duration_seconds, Utc::now());
        let deadlines: Vec<_> = room
            .disconnected_deadlines
            .iter()
            .map(|(player_id, deadline)| (*player_id, *deadline))
            .collect();
        let turn_timer = room.timer_state(Utc::now());
        if changed {
            self.store.save_room(&room).await?;
        }
        let room = Arc::new(Mutex::new(room));
        self.rooms.insert(id, room.clone());
        for (player_id, deadline) in deadlines {
            self.schedule_disconnect_expiry(id, player_id, deadline);
        }
        self.schedule_turn_expiry(turn_timer);
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
            if room.lock().await.code == normalized {
                return Ok(room);
            }
        }
        let mut room = self
            .store
            .room_by_code(&normalized)
            .await?
            .ok_or(GameError::RoomNotFound)?;
        let changed = room.ensure_runtime_state(self.settings.turn_duration_seconds, Utc::now());
        let id = room.id;
        let turn_timer = room.timer_state(Utc::now());
        if changed {
            self.store.save_room(&room).await?;
        }
        let room = Arc::new(Mutex::new(room));
        self.rooms.insert(id, room.clone());
        self.schedule_turn_expiry(turn_timer);
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
        let code = self.unique_room_code().await?;
        let room = GameRoom::new(
            code,
            input.name.trim().to_string(),
            input.visibility,
            session,
        )?;
        self.store.save_room(&room).await?;
        self.store
            .update_session_room(session.id, Some(room.id))
            .await?;
        self.rooms
            .insert(room.id, Arc::new(Mutex::new(room.clone())));
        Ok(room)
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
        let mut room = room.lock().await;
        room.join(session)?;
        self.store.save_room(&room).await?;
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
        self.store.save_room(&room).await?;
        self.store.update_session_room(session.id, None).await?;
        if room.game.as_ref().is_some_and(|game| game.result.is_some()) {
            self.cancel_turn_expiry(room.id);
        }
        Ok(room.clone())
    }

    pub async fn save_room(&self, room: &GameRoom) -> Result<(), GameError> {
        self.store.save_room(room).await
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
                self.hub.send(player.session_id, event);
            }
        }
    }

    pub fn broadcast_chat_message(&self, room: &GameRoom, message: &ChatMessage) {
        for player in &room.players {
            self.hub
                .send(player.session_id, ServerEvent::ChatMessage(message.clone()));
        }
    }

    pub fn broadcast_latest_chat_message(&self, room: &GameRoom) {
        if let Some(message) = room.chat_messages.last() {
            self.broadcast_chat_message(room, message);
        }
    }

    pub fn broadcast_chat_typing(&self, room: &GameRoom, event: &ChatTypingEvent) {
        for player in &room.players {
            if player.id != event.player_id {
                self.hub
                    .send(player.session_id, ServerEvent::ChatTyping(event.clone()));
            }
        }
    }

    pub fn broadcast_timer_state(&self, room: &GameRoom, event: fn(GameTimerState) -> ServerEvent) {
        if let Some(timer) = room.timer_state(Utc::now()) {
            for player in &room.players {
                self.hub.send(player.session_id, event(timer.clone()));
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
        if matches!(room.reconnect(session.id), Ok(true)) {
            let _ = self.store.save_room(&room).await;
            self.broadcast_latest_chat_message(&room);
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
            let _ = self.store.save_room(&room).await;
            self.broadcast_latest_chat_message(&room);
            self.broadcast_snapshots(&room, SnapshotEvent::PlayerDisconnected)
                .await;
            drop(room);

            self.schedule_disconnect_expiry(room_id, player_id, deadline);
            break;
        }
    }

    async fn restore_active_rooms(&self) -> Result<(), GameError> {
        for mut room in self.store.active_rooms().await? {
            let room_id = room.id;
            let changed =
                room.ensure_runtime_state(self.settings.turn_duration_seconds, Utc::now());
            let deadlines: Vec<_> = room
                .disconnected_deadlines
                .iter()
                .map(|(player_id, deadline)| (*player_id, *deadline))
                .collect();
            let turn_timer = room.timer_state(Utc::now());
            if changed {
                self.store.save_room(&room).await?;
            }
            self.rooms.insert(room_id, Arc::new(Mutex::new(room)));
            for (player_id, deadline) in deadlines {
                self.schedule_disconnect_expiry(room_id, player_id, deadline);
            }
            self.schedule_turn_expiry(turn_timer);
        }
        Ok(())
    }

    fn schedule_disconnect_expiry(
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
                let _ = state.store.save_room(&room).await;
                if room.status != crate::domain::RoomStatus::Playing {
                    state.cancel_turn_expiry(room.id);
                }
                if let Some(session_id) = expired_session_id {
                    let _ = state.store.update_session_room(session_id, None).await;
                }
                state.broadcast_latest_chat_message(&room);
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
            let delay = (deadline - Utc::now()).to_std().unwrap_or_default();
            tokio::time::sleep(delay).await;
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
            let record = room
                .expire_turn(
                    key.turn_number,
                    key.active_player_id,
                    key.deadline,
                    Utc::now(),
                )
                .unwrap_or(None);
            let Some(record) = record else {
                if state
                    .turn_timers
                    .get(&timer.room_id)
                    .is_some_and(|current| *current == key)
                {
                    state.cancel_turn_expiry(timer.room_id);
                }
                return;
            };
            let finished = record.winner_id.is_some();
            let next_timer = room.timer_state(Utc::now());
            let _ = state.store.save_room(&room).await;
            for player in &room.players {
                state
                    .hub
                    .send(player.session_id, ServerEvent::TurnExpired(record.clone()));
            }
            state.broadcast_latest_chat_message(&room);
            state
                .broadcast_snapshots(
                    &room,
                    if finished {
                        SnapshotEvent::GameFinished
                    } else {
                        SnapshotEvent::TurnChanged
                    },
                )
                .await;
            if finished {
                state.cancel_turn_expiry(timer.room_id);
            } else {
                state.broadcast_timer_state(&room, ServerEvent::TurnStarted);
                state.schedule_turn_expiry(next_timer);
            }
        });
    }

    pub fn cancel_turn_expiry(&self, room_id: Uuid) {
        self.turn_timers.remove(&room_id);
    }

    pub async fn enqueue_matchmaking(
        &self,
        session: UserSession,
    ) -> Result<Option<GameRoom>, GameError> {
        if session.current_room_id.is_some() {
            return Err(GameError::AlreadyJoined);
        }
        let mut queue = self.matchmaking.lock().await;
        queue.retain(|entry| entry.session.id != session.id);
        if let Some(opponent) = queue.pop_front() {
            drop(queue);
            let mut room = self
                .create_room(
                    &opponent.session,
                    CreateRoomInput {
                        name: "신속 교전".to_string(),
                        visibility: RoomVisibility::Private,
                    },
                )
                .await?;
            let room_ref = self.room(room.id).await?;
            {
                let mut locked = room_ref.lock().await;
                locked.join(&session)?;
                self.store.save_room(&locked).await?;
                self.store
                    .update_session_room(session.id, Some(locked.id))
                    .await?;
                room = locked.clone();
            }
            self.broadcast_snapshots(&room, SnapshotEvent::PlayerJoined)
                .await;
            self.broadcast_latest_chat_message(&room);
            Ok(Some(room))
        } else {
            queue.push_back(QueuedSession {
                session,
                queued_at: Utc::now(),
            });
            Ok(None)
        }
    }

    pub async fn cancel_matchmaking(&self, session_id: Uuid) -> bool {
        let mut queue = self.matchmaking.lock().await;
        let before = queue.len();
        queue.retain(|entry| entry.session.id != session_id);
        before != queue.len()
    }

    pub async fn matchmaking_time(&self, session_id: Uuid) -> Option<chrono::DateTime<Utc>> {
        self.matchmaking
            .lock()
            .await
            .iter()
            .find(|entry| entry.session.id == session_id)
            .map(|entry| entry.queued_at)
    }

    async fn unique_room_code(&self) -> Result<String, GameError> {
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

fn validate_nickname(nickname: &str) -> Result<(), GameError> {
    let count = nickname.chars().count();
    let valid = (2..=16).contains(&count)
        && nickname.chars().all(|character| {
            character.is_alphanumeric() || character == ' ' || character == '_' || character == '-'
        });
    if valid {
        Ok(())
    } else {
        Err(GameError::InvalidNickname)
    }
}

pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Clone)]
struct ConnectionEntry {
    connection_id: Uuid,
    sender: mpsc::UnboundedSender<ServerEvent>,
}

#[derive(Debug, Clone, Default)]
pub struct ConnectionHub {
    connections: Arc<DashMap<Uuid, ConnectionEntry>>,
}

impl ConnectionHub {
    pub fn connect(&self, session_id: Uuid, sender: mpsc::UnboundedSender<ServerEvent>) -> Uuid {
        let connection_id = Uuid::new_v4();
        self.connections.insert(
            session_id,
            ConnectionEntry {
                connection_id,
                sender,
            },
        );
        connection_id
    }

    pub fn disconnect_if_current(&self, session_id: Uuid, connection_id: Uuid) -> bool {
        let is_current = self
            .connections
            .get(&session_id)
            .map(|entry| entry.connection_id == connection_id)
            .unwrap_or(false);
        if is_current {
            self.connections.remove(&session_id);
        }
        is_current
    }

    pub fn send(&self, session_id: Uuid, event: ServerEvent) {
        if let Some(connection) = self.connections.get(&session_id) {
            let _ = connection.sender.send(event);
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    let origins: Vec<HeaderValue> = state
        .settings
        .allowed_origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(origins)
        .allow_credentials(true)
        .allow_headers([
            http::header::CONTENT_TYPE,
            http::header::AUTHORIZATION,
            http::header::ACCEPT,
        ])
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS]);

    Router::new()
        .nest("/api", api::router())
        .route("/ws", get(ws::websocket_handler))
        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(15),
        ))
        .layer(CompressionLayer::new())
        .layer(CatchPanicLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Coordinate, Orientation, RoomStatus, ShipKind, ShipPlacement};

    fn session(nickname: &str) -> UserSession {
        let now = Utc::now();
        UserSession {
            id: Uuid::new_v4(),
            nickname: nickname.to_string(),
            token_hash: Uuid::new_v4().to_string(),
            created_at: now,
            last_seen_at: now,
            current_room_id: None,
        }
    }

    fn fleet(first_row: u8) -> Vec<ShipPlacement> {
        [
            (ShipKind::Carrier, 0_u8),
            (ShipKind::Battleship, 1),
            (ShipKind::Cruiser, 2),
            (ShipKind::Submarine, 3),
            (ShipKind::Destroyer, 4),
        ]
        .into_iter()
        .map(|(kind, offset)| ShipPlacement {
            kind,
            origin: Coordinate {
                row: first_row + offset,
                col: 0,
            },
            orientation: Orientation::Horizontal,
        })
        .collect()
    }

    #[tokio::test]
    async fn restart_reclaims_an_already_expired_persisted_turn_once() {
        let first = session("Alpha");
        let second = session("Bravo");
        let mut room = GameRoom::new(
            "RST234".to_string(),
            "Restart recovery".to_string(),
            RoomVisibility::Private,
            &first,
        )
        .unwrap();
        room.join(&second).unwrap();
        let first_player_id = room.player_for_session(first.id).unwrap().id;
        let second_player_id = room.player_for_session(second.id).unwrap().id;
        room.set_lobby_ready(first.id, Uuid::new_v4(), first_player_id, true)
            .unwrap();
        room.set_lobby_ready(second.id, Uuid::new_v4(), second_player_id, true)
            .unwrap();
        room.start_placement(first.id, Uuid::new_v4(), first_player_id, room.version)
            .unwrap();
        room.place_ships(first.id, fleet(0)).unwrap();
        room.place_ships(second.id, fleet(5)).unwrap();
        room.confirm_placement(first.id, &fleet(0), 1).unwrap();
        room.confirm_placement(second.id, &fleet(5), 1).unwrap();
        let original_turn = room.game.as_ref().unwrap().turn_number;
        room.game.as_mut().unwrap().turn_deadline_at =
            Some(Utc::now() - chrono::Duration::seconds(1));

        let store = Arc::new(MemoryStore::default());
        store.save_room(&room).await.unwrap();
        let settings = Settings {
            storage_mode: StorageMode::Memory,
            turn_duration_seconds: 1,
            ..Settings::default()
        };
        let state = AppState::with_store(settings, store.clone());
        state.restore_active_rooms().await.unwrap();

        for _ in 0..40 {
            let recovered = store.room_by_id(room.id).await.unwrap().unwrap();
            if recovered.game.as_ref().unwrap().turn_number > original_turn {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let recovered = store.room_by_id(room.id).await.unwrap().unwrap();
        assert_eq!(recovered.status, RoomStatus::Playing);
        assert_eq!(
            recovered.game.as_ref().unwrap().turn_number,
            original_turn + 1
        );
        assert_eq!(
            recovered
                .game
                .as_ref()
                .unwrap()
                .total_timeout_counts
                .values()
                .sum::<u32>(),
            1
        );
    }
}
