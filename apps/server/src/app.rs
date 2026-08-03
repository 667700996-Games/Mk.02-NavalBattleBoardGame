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
    domain::{GameRoom, RoomVisibility, UserSession},
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

#[derive(Debug, Clone, Copy)]
pub enum SnapshotEvent {
    RoomUpdated,
    PlayerJoined,
    PlayerLeft,
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
        Ok(Self {
            settings: Arc::new(settings),
            store,
            rooms: Arc::new(DashMap::new()),
            hub: ConnectionHub::default(),
            matchmaking: Arc::new(Mutex::new(VecDeque::new())),
        })
    }

    pub fn with_store(settings: Settings, store: Arc<dyn GameStore>) -> Self {
        Self {
            settings: Arc::new(settings),
            store,
            rooms: Arc::new(DashMap::new()),
            hub: ConnectionHub::default(),
            matchmaking: Arc::new(Mutex::new(VecDeque::new())),
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
        let room = self
            .store
            .room_by_id(id)
            .await?
            .ok_or(GameError::RoomNotFound)?;
        let room = Arc::new(Mutex::new(room));
        self.rooms.insert(id, room.clone());
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
        let room = self
            .store
            .room_by_code(&normalized)
            .await?
            .ok_or(GameError::RoomNotFound)?;
        let id = room.id;
        let room = Arc::new(Mutex::new(room));
        self.rooms.insert(id, room.clone());
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

    pub async fn restore_connection(&self, session: &UserSession) {
        let Some(room_id) = session.current_room_id else {
            return;
        };
        let Ok(room) = self.room(room_id).await else {
            return;
        };
        let mut room = room.lock().await;
        if room.reconnect(session.id).is_ok() {
            let _ = self.store.save_room(&room).await;
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
            let Ok(_) = room.disconnect(session_id, grace) else {
                continue;
            };
            let room_id = room.id;
            let player_id = match room.player_for_session(session_id) {
                Ok(player) => player.id,
                Err(_) => continue,
            };
            let _ = self.store.save_room(&room).await;
            self.broadcast_snapshots(&room, SnapshotEvent::PlayerDisconnected)
                .await;
            drop(room);

            let state = self.clone();
            tokio::spawn(async move {
                tokio::time::sleep(state.settings.reconnect_grace).await;
                let Ok(room_ref) = state.room(room_id).await else {
                    return;
                };
                let mut room = room_ref.lock().await;
                if room
                    .expire_disconnect(player_id, Utc::now())
                    .unwrap_or(false)
                {
                    let _ = state.store.save_room(&room).await;
                    state
                        .broadcast_snapshots(&room, SnapshotEvent::GameFinished)
                        .await;
                }
            });
            break;
        }
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
