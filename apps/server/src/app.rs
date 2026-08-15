use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    http::{HeaderValue, Method},
    routing::get,
};
use axum_extra::extract::CookieJar;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use dashmap::DashMap;
use futures_util::StreamExt;
use rand::RngCore;
use redis::{AsyncCommands, aio::ConnectionManager};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore, mpsc};
use tower_http::{
    catch_panic::CatchPanicLayer, compression::CompressionLayer, cors::CorsLayer,
    timeout::TimeoutLayer, trace::TraceLayer,
};
use uuid::Uuid;

const DISTRIBUTED_EVENT_CHANNEL: &str = "mk01:server-events:v1";

use crate::{
    api,
    config::{Settings, StorageMode},
    domain::{ChatMessage, ChatTypingEvent, GameRoom, GameTimerState, RoomVisibility, UserSession},
    error::GameError,
    protocol::{CreateRoomInput, ServerEvent},
    rate_limit::FixedWindowRateLimiter,
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
    api_rate_limiter: FixedWindowRateLimiter,
    request_ip_rate_limiter: FixedWindowRateLimiter,
    session_creation_rate_limiter: FixedWindowRateLimiter,
    websocket_event_rate_limiter: FixedWindowRateLimiter,
    websocket_slots: Arc<Semaphore>,
    instance_id: Uuid,
    distributed_event_publisher: Option<ConnectionManager>,
    coordination_healthy: Arc<AtomicBool>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DistributedEventEnvelope {
    origin_instance_id: Uuid,
    session_id: Uuid,
    event_json: String,
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
        let api_rate_limiter = FixedWindowRateLimiter::new(
            Duration::from_secs(60),
            settings.api_requests_per_minute,
            100_000,
        );
        let request_ip_rate_limiter = FixedWindowRateLimiter::new(
            Duration::from_secs(60),
            settings.http_requests_per_minute_per_ip,
            50_000,
        );
        let session_creation_rate_limiter = FixedWindowRateLimiter::new(
            Duration::from_secs(60),
            settings.session_creations_per_minute,
            50_000,
        );
        let websocket_event_rate_limiter = FixedWindowRateLimiter::new(
            Duration::from_secs(1),
            settings.websocket_events_per_second,
            100_000,
        );
        let websocket_slots = Arc::new(Semaphore::new(settings.max_websocket_connections));
        let instance_id = Uuid::new_v4();
        let (distributed_event_publisher, distributed_event_subscriber) =
            connect_distributed_events(&settings).await?;
        let coordination_healthy = Arc::new(AtomicBool::new(
            settings.storage_mode == StorageMode::Memory
                || distributed_event_publisher.is_some()
                || !settings.distributed_coordination_required,
        ));
        let state = Self {
            settings: Arc::new(settings),
            store,
            rooms: Arc::new(DashMap::new()),
            hub: ConnectionHub::default(),
            matchmaking: Arc::new(Mutex::new(VecDeque::new())),
            turn_timers: Arc::new(DashMap::new()),
            api_rate_limiter,
            request_ip_rate_limiter,
            session_creation_rate_limiter,
            websocket_event_rate_limiter,
            websocket_slots,
            instance_id,
            distributed_event_publisher,
            coordination_healthy,
        };
        if let Some((client, subscriber)) = distributed_event_subscriber {
            state.start_distributed_event_subscriber(client, subscriber);
        }
        state.restore_active_rooms().await?;
        state.start_turn_expiry_watchdog();
        Ok(state)
    }

    pub fn with_store(settings: Settings, store: Arc<dyn GameStore>) -> Self {
        let api_rate_limiter = FixedWindowRateLimiter::new(
            Duration::from_secs(60),
            settings.api_requests_per_minute,
            100_000,
        );
        let request_ip_rate_limiter = FixedWindowRateLimiter::new(
            Duration::from_secs(60),
            settings.http_requests_per_minute_per_ip,
            50_000,
        );
        let session_creation_rate_limiter = FixedWindowRateLimiter::new(
            Duration::from_secs(60),
            settings.session_creations_per_minute,
            50_000,
        );
        let websocket_event_rate_limiter = FixedWindowRateLimiter::new(
            Duration::from_secs(1),
            settings.websocket_events_per_second,
            100_000,
        );
        let websocket_slots = Arc::new(Semaphore::new(settings.max_websocket_connections));
        let coordination_healthy = Arc::new(AtomicBool::new(
            settings.storage_mode == StorageMode::Memory
                || !settings.distributed_coordination_required,
        ));
        Self {
            settings: Arc::new(settings),
            store,
            rooms: Arc::new(DashMap::new()),
            hub: ConnectionHub::default(),
            matchmaking: Arc::new(Mutex::new(VecDeque::new())),
            turn_timers: Arc::new(DashMap::new()),
            api_rate_limiter,
            request_ip_rate_limiter,
            session_creation_rate_limiter,
            websocket_event_rate_limiter,
            websocket_slots,
            instance_id: Uuid::new_v4(),
            distributed_event_publisher: None,
            coordination_healthy,
        }
    }

    pub async fn health_check(&self) -> Result<(), GameError> {
        self.store.health_check().await?;
        if self.settings.distributed_coordination_required
            && !self.coordination_healthy.load(Ordering::Relaxed)
        {
            return Err(GameError::StorageUnavailable);
        }
        Ok(())
    }

    pub async fn send_to_session(&self, session_id: Uuid, event: ServerEvent) {
        let Ok(event_json) = serde_json::to_string(&event) else {
            tracing::error!(%session_id, "server event serialization failed");
            return;
        };
        self.hub.send_serialized(session_id, event_json.clone());

        let Some(mut publisher) = self.distributed_event_publisher.clone() else {
            return;
        };
        let envelope = DistributedEventEnvelope {
            origin_instance_id: self.instance_id,
            session_id,
            event_json,
        };
        let Ok(payload) = serde_json::to_string(&envelope) else {
            tracing::error!(%session_id, "distributed event serialization failed");
            return;
        };
        match publisher
            .publish::<_, _, usize>(DISTRIBUTED_EVENT_CHANNEL, payload)
            .await
        {
            Ok(_) => self.coordination_healthy.store(true, Ordering::Relaxed),
            Err(error) => {
                self.coordination_healthy.store(false, Ordering::Relaxed);
                tracing::error!(%error, %session_id, "distributed event publish failed");
            }
        }
    }

    fn start_distributed_event_subscriber(
        &self,
        client: redis::Client,
        subscriber: redis::aio::PubSub,
    ) {
        let hub = self.hub.clone();
        let instance_id = self.instance_id;
        let coordination_healthy = self.coordination_healthy.clone();
        tokio::spawn(async move {
            let mut initial = Some(subscriber);
            loop {
                let mut subscriber = if let Some(subscriber) = initial.take() {
                    subscriber
                } else {
                    match client.get_async_pubsub().await {
                        Ok(mut subscriber) => {
                            if let Err(error) = subscriber.subscribe(DISTRIBUTED_EVENT_CHANNEL).await
                            {
                                coordination_healthy.store(false, Ordering::Relaxed);
                                tracing::error!(%error, "distributed event resubscribe failed");
                                tokio::time::sleep(Duration::from_secs(1)).await;
                                continue;
                            }
                            subscriber
                        }
                        Err(error) => {
                            coordination_healthy.store(false, Ordering::Relaxed);
                            tracing::error!(%error, "distributed event subscriber reconnect failed");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    }
                };
                coordination_healthy.store(true, Ordering::Relaxed);
                let mut messages = subscriber.on_message();
                while let Some(message) = messages.next().await {
                    let Ok(payload) = message.get_payload::<String>() else {
                        continue;
                    };
                    let Ok(envelope) = serde_json::from_str::<DistributedEventEnvelope>(&payload)
                    else {
                        continue;
                    };
                    if envelope.origin_instance_id != instance_id {
                        hub.send_serialized(envelope.session_id, envelope.event_json);
                    }
                }
                coordination_healthy.store(false, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }

    pub fn enforce_api_rate_limit(&self, session_id: Uuid) -> Result<(), GameError> {
        self.api_rate_limiter
            .check(session_id.to_string())
            .then_some(())
            .ok_or(GameError::RateLimited)
    }

    pub fn enforce_ip_rate_limit(
        &self,
        headers: &http::HeaderMap,
        direct_address: SocketAddr,
    ) -> Result<(), GameError> {
        let client_key = self.client_rate_limit_key(headers, direct_address);
        self.request_ip_rate_limiter
            .check(client_key)
            .then_some(())
            .ok_or(GameError::RateLimited)
    }

    pub fn client_rate_limit_key(
        &self,
        headers: &http::HeaderMap,
        direct_address: SocketAddr,
    ) -> String {
        if self.settings.trust_proxy_headers {
            let forwarded_ip = headers
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(',').next())
                .map(str::trim)
                .and_then(|value| value.parse::<std::net::IpAddr>().ok())
                .or_else(|| {
                    headers
                        .get("x-real-ip")
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.parse::<std::net::IpAddr>().ok())
                });
            if let Some(ip) = forwarded_ip {
                return ip.to_string();
            }
        }
        direct_address.ip().to_string()
    }

    pub fn enforce_session_creation_rate_limit(&self, client_key: &str) -> Result<(), GameError> {
        self.session_creation_rate_limiter
            .check(client_key)
            .then_some(())
            .ok_or(GameError::RateLimited)
    }

    pub fn allow_websocket_event(&self, session_id: Uuid) -> bool {
        self.websocket_event_rate_limiter
            .check(session_id.to_string())
    }

    pub fn websocket_send_queue_capacity(&self) -> usize {
        self.settings.websocket_send_queue_capacity
    }

    pub fn try_acquire_websocket_slot(&self) -> Result<OwnedSemaphorePermit, GameError> {
        self.websocket_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| GameError::RateLimited)
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
            self.save_room(&mut room).await?;
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
            self.save_room(&mut room).await?;
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
        let mut room = GameRoom::new(
            code,
            input.name.trim().to_string(),
            input.visibility,
            session,
        )?;
        self.save_room(&mut room).await?;
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
        match self.store.save_room(room).await {
            Ok(()) => Ok(()),
            Err(GameError::VersionConflict) => {
                if let Ok(Some(latest)) = self.store.room_by_id_authoritative(room.id).await {
                    *room = latest;
                }
                Err(GameError::VersionConflict)
            }
            Err(error) => Err(error),
        }
    }

    pub async fn revoke_session(
        &self,
        session: &UserSession,
    ) -> Result<Option<GameRoom>, GameError> {
        self.cancel_matchmaking(session.id).await;
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
            self.send_to_session(
                player.session_id,
                ServerEvent::ChatMessage(message.clone()),
            )
            .await;
        }
    }

    pub async fn broadcast_latest_chat_message(&self, room: &GameRoom) {
        if let Some(message) = room.chat_messages.last() {
            self.broadcast_chat_message(room, message).await;
        }
    }

    pub async fn broadcast_chat_typing(&self, room: &GameRoom, event: &ChatTypingEvent) {
        for player in &room.players {
            if player.id != event.player_id {
                self.send_to_session(player.session_id, ServerEvent::ChatTyping(event.clone()))
                    .await;
            }
        }
    }

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
        if matches!(room.reconnect(session.id), Ok(true)) {
            let _ = self.save_room(&mut room).await;
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
            let _ = self.save_room(&mut room).await;
            self.broadcast_latest_chat_message(&room).await;
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
                self.save_room(&mut room).await?;
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
                let _ = state.save_room(&mut room).await;
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

    fn start_turn_expiry_watchdog(&self) {
        let state = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let rooms: Vec<_> = state
                    .rooms
                    .iter()
                    .map(|entry| entry.value().clone())
                    .collect();
                for room_ref in rooms {
                    let mut room = room_ref.lock().await;
                    let Some(timer) = room.timer_state(Utc::now()) else {
                        continue;
                    };
                    let Some(deadline) = timer.turn_deadline_at else {
                        continue;
                    };
                    if Utc::now() < deadline {
                        continue;
                    }
                    let key = TurnTimerKey {
                        turn_number: timer.turn_number,
                        active_player_id: timer.active_player_id,
                        deadline,
                    };
                    state.resolve_turn_expiry(&mut room, &key).await;
                }
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
        }
        true
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
                self.save_room(&mut locked).await?;
                self.store
                    .update_session_room(session.id, Some(locked.id))
                    .await?;
                room = locked.clone();
            }
            self.broadcast_snapshots(&room, SnapshotEvent::PlayerJoined)
                .await;
            self.broadcast_latest_chat_message(&room).await;
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

async fn connect_distributed_events(
    settings: &Settings,
) -> Result<
    (
        Option<ConnectionManager>,
        Option<(redis::Client, redis::aio::PubSub)>,
    ),
    GameError,
> {
    if settings.storage_mode == StorageMode::Memory {
        return Ok((None, None));
    }
    let connection = async {
        let client = redis::Client::open(settings.redis_url.as_str())?;
        let publisher = ConnectionManager::new(client.clone()).await?;
        let mut subscriber = client.get_async_pubsub().await?;
        subscriber.subscribe(DISTRIBUTED_EVENT_CHANNEL).await?;
        Ok::<_, redis::RedisError>((publisher, client, subscriber))
    }
    .await;
    match connection {
        Ok((publisher, client, subscriber)) => {
            Ok((Some(publisher), Some((client, subscriber))))
        }
        Err(error) if settings.distributed_coordination_required => {
            tracing::error!(%error, "required distributed event coordination unavailable");
            Err(GameError::StorageUnavailable)
        }
        Err(error) => {
            tracing::warn!(%error, "distributed event coordination disabled; running single-instance only");
            Ok((None, None))
        }
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
    sender: mpsc::Sender<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ConnectionHub {
    connections: Arc<DashMap<Uuid, ConnectionEntry>>,
}

impl ConnectionHub {
    pub fn connect(&self, session_id: Uuid, sender: mpsc::Sender<String>) -> Uuid {
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
        let Ok(serialized) = serde_json::to_string(&event) else {
            tracing::error!(%session_id, "server event serialization failed");
            return;
        };
        self.send_serialized(session_id, serialized);
    }

    pub fn send_serialized(&self, session_id: Uuid, serialized: String) {
        let Some(connection) = self.connections.get(&session_id) else {
            return;
        };
        let connection_id = connection.connection_id;
        let send_result = connection.sender.try_send(serialized);
        drop(connection);
        if let Err(error) = send_result {
            let reason = match error {
                mpsc::error::TrySendError::Full(_) => "websocket slow consumer disconnected",
                mpsc::error::TrySendError::Closed(_) => "websocket closed consumer removed",
            };
            if self.disconnect_if_current(session_id, connection_id) {
                tracing::warn!(%session_id, %connection_id, %reason);
            }
        }
    }

    pub fn close(&self, session_id: Uuid) -> bool {
        self.connections.remove(&session_id).is_some()
    }

    pub fn len(&self) -> usize {
        self.connections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
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

    let rate_limit_state = state.clone();
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
        .layer(axum::middleware::from_fn_with_state(
            rate_limit_state,
            request_ip_rate_limit,
        ))
        .layer(axum::middleware::from_fn(security_headers))
        .layer(cors)
        .with_state(state)
}

async fn request_ip_rate_limit(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::ConnectInfo(address): axum::extract::ConnectInfo<SocketAddr>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, GameError> {
    state.enforce_ip_rate_limit(request.headers(), address)?;
    Ok(next.run(request).await)
}

async fn security_headers(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        http::header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'; form-action 'self'; img-src 'self' data: blob:; font-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self' ws: wss:; worker-src 'self' blob:",
        ),
    );
    headers.insert(
        http::header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );
    headers.insert(
        http::header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        http::header::X_FRAME_OPTIONS,
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        http::header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        http::header::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
    );
    headers.insert(
        http::header::HeaderName::from_static("cross-origin-opener-policy"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        http::header::HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-origin"),
    );
    response
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
        store.save_room(&mut room).await.unwrap();
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

    #[tokio::test]
    async fn turn_deadline_advances_without_a_client_event() {
        let first = session("Alpha");
        let second = session("Bravo");
        let mut room = GameRoom::new(
            "TMR234".to_string(),
            "Timer progression".to_string(),
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
        let original_player = room.game.as_ref().unwrap().current_player_id;
        let store = Arc::new(MemoryStore::default());
        store.save_room(&mut room).await.unwrap();
        let settings = Settings {
            storage_mode: StorageMode::Memory,
            turn_duration_seconds: 1,
            ..Settings::default()
        };
        let state = AppState::with_store(settings, store);
        state
            .rooms
            .insert(room.id, Arc::new(Mutex::new(room.clone())));
        state.start_turn_expiry_watchdog();

        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let active_room = state.room(room.id).await.unwrap();
            let active_game = active_room.lock().await.game.clone().unwrap();
            if active_game.turn_number > original_turn {
                assert_ne!(active_game.current_player_id, original_player);
                assert_eq!(active_game.total_timeout_counts[&original_player], 1);
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "turn did not expire on the server"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn websocket_capacity_and_slow_consumers_are_bounded() {
        let settings = Settings {
            max_websocket_connections: 1,
            ..Settings::default()
        };
        let state = AppState::with_store(settings, Arc::new(MemoryStore::default()));
        let permit = state.try_acquire_websocket_slot().unwrap();
        assert_eq!(
            state.try_acquire_websocket_slot().unwrap_err(),
            GameError::RateLimited
        );
        drop(permit);
        assert!(state.try_acquire_websocket_slot().is_ok());

        let hub = ConnectionHub::default();
        let session_id = Uuid::new_v4();
        let (sender, _receiver) = mpsc::channel(1);
        hub.connect(session_id, sender);
        let heartbeat = || {
            ServerEvent::Heartbeat(crate::protocol::HeartbeatResponse {
                server_time: Utc::now(),
            })
        };
        hub.send(session_id, heartbeat());
        assert_eq!(hub.len(), 1);
        hub.send(session_id, heartbeat());
        assert!(hub.is_empty());
    }
}
