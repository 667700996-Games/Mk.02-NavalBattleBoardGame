use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    http::{HeaderName, HeaderValue, Method, StatusCode},
    response::IntoResponse,
    routing::get,
};
use axum_extra::extract::CookieJar;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Datelike, Utc};
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
const DISTRIBUTED_EVENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const ROOM_AUTHORITY_LEASE_DURATION: Duration = Duration::from_secs(5);
const DISTRIBUTED_RATE_LIMIT_SCRIPT: &str = r#"
local current = redis.call('INCR', KEYS[1])
if current == 1 then
  redis.call('PEXPIRE', KEYS[1], ARGV[2])
end
if current <= tonumber(ARGV[1]) then
  return 1
end
return 0
"#;

use crate::{
    api,
    config::{Settings, StorageMode},
    domain::{
        AccountSession, AchievementProgress, ActivePenalty, AiDifficulty, AttackOutcome,
        ChatMessage, ChatTypingEvent, Coordinate, FinishReason, GameRoom, GameTimerState,
        IntegritySignalKind, IntegritySignalPage, LiveContentPayload, LiveContentRevision,
        LiveContentValidation, LiveContentView, MatchmakingCriteria, MatchmakingPool,
        MatchmakingPreferences, MatchmakingQuality, MatchmakingSearchWindow, MissionCadence,
        MissionProgress, ModerationAction, ModerationActionKind, ModerationCasePage,
        NewIntegritySignal, NewModerationAction, NewPlayerReport, NewSupportAction, Orientation,
        PlayerAccount, PlayerKind, PlayerProgression, PlayerReportReceipt,
        RANKED_LEADERBOARD_DEFAULT_LIMIT, RANKED_LEADERBOARD_FINALIZATION_HOURS,
        RANKED_LEADERBOARD_MAX_LIMIT, RankedLeaderboardPage, RankedMatchContext, RankedProfile,
        ReportCategory, ReportStatus, RoomVisibility, ShipKind, ShipPlacement, SocialRelationship,
        SupportAccountSnapshot, SupportAction, SupportActionKind, UserSession,
        baseline_live_content, ranked_season_key,
    },
    error::GameError,
    protocol::{
        CreateRoomInput, FunnelFailureReason, FunnelOutcome, FunnelStage, NegotiatedProtocol,
        PROTOCOL_CAPABILITIES, PROTOCOL_CAPABILITIES_HEADER, PROTOCOL_MAX_VERSION_HEADER,
        PROTOCOL_MIN_VERSION_HEADER, PROTOCOL_VERSION_HEADER, ProtocolError, RumDeviceTier,
        RumMetric, RumRoute, ServerEvent, negotiate_protocol_version,
    },
    rate_limit::FixedWindowRateLimiter,
    store::{
        AccountDeletionScope, AccountDeletionStats, GameHistoryItem, GameStore,
        MatchmakingQueueEntry, MatchmakingQueueStats, MemoryStore, MissionReward,
        PostgresRedisStore,
    },
    ws,
};

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub store: Arc<dyn GameStore>,
    pub rooms: Arc<DashMap<Uuid, Arc<Mutex<GameRoom>>>>,
    pub hub: ConnectionHub,
    pub metrics: Arc<ServerMetrics>,
    turn_timers: Arc<DashMap<Uuid, TurnTimerKey>>,
    api_rate_limiter: FixedWindowRateLimiter,
    request_ip_rate_limiter: FixedWindowRateLimiter,
    session_creation_rate_limiter: FixedWindowRateLimiter,
    websocket_event_rate_limiter: FixedWindowRateLimiter,
    websocket_slots: Arc<Semaphore>,
    instance_id: Uuid,
    distributed_event_publisher: Option<ConnectionManager>,
    coordination_healthy: Arc<AtomicBool>,
    integrity_signal_cooldowns: Arc<DashMap<(Uuid, IntegritySignalKind), std::time::Instant>>,
}

#[derive(Debug, Clone)]
pub struct MatchmakingOutcome {
    pub room: Option<GameRoom>,
    pub queued_at: Option<chrono::DateTime<Utc>>,
    pub criteria: MatchmakingCriteria,
    pub search_window: MatchmakingSearchWindow,
    pub quality: Option<MatchmakingQuality>,
}

mod accounts;
mod connections;
mod live_content;
mod matchmaking;
mod metrics;
mod rooms;
mod router;
mod safety;
mod timers;

pub use accounts::hash_token;
pub use connections::ConnectionHub;
pub use metrics::{CommandTransport, ServerMetrics};
pub use router::build_router;
impl std::fmt::Debug for AppState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppState")
            .field("storage", &self.store.kind())
            .field("cached_rooms", &self.rooms.len())
            .finish()
    }
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
    #[serde(default)]
    close_after_delivery: bool,
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
            metrics: Arc::new(ServerMetrics::default()),
            turn_timers: Arc::new(DashMap::new()),
            api_rate_limiter,
            request_ip_rate_limiter,
            session_creation_rate_limiter,
            websocket_event_rate_limiter,
            websocket_slots,
            instance_id,
            distributed_event_publisher,
            coordination_healthy,
            integrity_signal_cooldowns: Arc::new(DashMap::new()),
        };
        if let Some((client, subscriber)) = distributed_event_subscriber {
            state.start_distributed_event_subscriber(client, subscriber);
        }
        state.run_retention_sweep().await?;
        state.start_retention_worker();
        state.restore_active_rooms().await?;
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
            metrics: Arc::new(ServerMetrics::default()),
            turn_timers: Arc::new(DashMap::new()),
            api_rate_limiter,
            request_ip_rate_limiter,
            session_creation_rate_limiter,
            websocket_event_rate_limiter,
            websocket_slots,
            instance_id: Uuid::new_v4(),
            distributed_event_publisher: None,
            coordination_healthy,
            integrity_signal_cooldowns: Arc::new(DashMap::new()),
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

    async fn run_retention_sweep(&self) -> Result<(), GameError> {
        let to_chrono = |duration: Duration| {
            chrono::Duration::from_std(duration).unwrap_or(chrono::Duration::MAX)
        };
        let now = Utc::now();
        let stats = self
            .store
            .prune_expired_data(
                now - to_chrono(self.settings.session_ttl),
                now - to_chrono(self.settings.completed_room_retention),
                now - to_chrono(self.settings.matchmaking_entry_ttl),
                now - to_chrono(self.settings.moderation_retention),
                now - to_chrono(self.settings.integrity_signal_retention),
            )
            .await?;
        self.metrics
            .retention_sessions_deleted
            .fetch_add(stats.sessions_deleted, Ordering::Relaxed);
        self.metrics
            .retention_rooms_deleted
            .fetch_add(stats.rooms_deleted, Ordering::Relaxed);
        self.metrics
            .retention_matchmaking_deleted
            .fetch_add(stats.matchmaking_entries_deleted, Ordering::Relaxed);
        self.metrics
            .retention_moderation_deleted
            .fetch_add(stats.moderation_cases_deleted, Ordering::Relaxed);
        self.metrics
            .retention_integrity_deleted
            .fetch_add(stats.integrity_signals_deleted, Ordering::Relaxed);
        if stats.sessions_deleted
            + stats.rooms_deleted
            + stats.matchmaking_entries_deleted
            + stats.moderation_cases_deleted
            + stats.integrity_signals_deleted
            > 0
        {
            tracing::info!(
                sessions_deleted = stats.sessions_deleted,
                rooms_deleted = stats.rooms_deleted,
                matchmaking_entries_deleted = stats.matchmaking_entries_deleted,
                moderation_cases_deleted = stats.moderation_cases_deleted,
                integrity_signals_deleted = stats.integrity_signals_deleted,
                "retention sweep completed"
            );
        }
        Ok(())
    }

    fn start_retention_worker(&self) {
        let state = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(state.settings.retention_sweep_interval);
            interval.tick().await;
            loop {
                interval.tick().await;
                if let Err(error) = state.run_retention_sweep().await {
                    tracing::error!(error_code = error.code(), "retention sweep failed");
                }
            }
        });
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
            close_after_delivery: false,
        };
        let Ok(payload) = serde_json::to_string(&envelope) else {
            tracing::error!(%session_id, "distributed event serialization failed");
            return;
        };
        match publisher
            .publish::<_, _, usize>(DISTRIBUTED_EVENT_CHANNEL, payload)
            .await
        {
            Ok(_) => {
                self.metrics
                    .distributed_events_published
                    .fetch_add(1, Ordering::Relaxed);
                self.coordination_healthy.store(true, Ordering::Relaxed);
            }
            Err(error) => {
                self.metrics
                    .distributed_event_failures
                    .fetch_add(1, Ordering::Relaxed);
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
                            if let Err(error) =
                                subscriber.subscribe(DISTRIBUTED_EVENT_CHANNEL).await
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
                        if envelope.close_after_delivery {
                            hub.close(envelope.session_id);
                        }
                    }
                }
                coordination_healthy.store(false, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }

    async fn close_session_everywhere(&self, session_id: Uuid, error: GameError) {
        let event = ServerEvent::Error(ProtocolError {
            code: error.code().to_string(),
            message: error.to_string(),
            retryable: false,
            request_id: Uuid::new_v4(),
        });
        let Ok(event_json) = serde_json::to_string(&event) else {
            self.hub.close(session_id);
            return;
        };
        self.hub.send_serialized(session_id, event_json.clone());
        self.hub.close(session_id);
        let Some(mut publisher) = self.distributed_event_publisher.clone() else {
            return;
        };
        let envelope = DistributedEventEnvelope {
            origin_instance_id: self.instance_id,
            session_id,
            event_json,
            close_after_delivery: true,
        };
        let Ok(payload) = serde_json::to_string(&envelope) else {
            return;
        };
        if let Err(error) = publisher
            .publish::<_, _, usize>(DISTRIBUTED_EVENT_CHANNEL, payload)
            .await
        {
            self.metrics
                .distributed_event_failures
                .fetch_add(1, Ordering::Relaxed);
            self.coordination_healthy.store(false, Ordering::Relaxed);
            tracing::error!(%error, %session_id, "distributed session close publish failed");
        } else {
            self.metrics
                .distributed_events_published
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    pub async fn enforce_api_rate_limit(&self, session_id: Uuid) -> Result<(), GameError> {
        let key = session_id.to_string();
        self.api_rate_limiter
            .check(&key)
            .then_some(())
            .ok_or(GameError::RateLimited)?;
        self.enforce_distributed_rate_limit(
            "api-session",
            &key,
            self.settings.api_requests_per_minute,
            Duration::from_secs(60),
        )
        .await
    }

    pub async fn enforce_ip_rate_limit(
        &self,
        headers: &http::HeaderMap,
        direct_address: SocketAddr,
    ) -> Result<(), GameError> {
        let client_key = self.client_rate_limit_key(headers, direct_address);
        self.request_ip_rate_limiter
            .check(&client_key)
            .then_some(())
            .ok_or(GameError::RateLimited)?;
        self.enforce_distributed_rate_limit(
            "http-ip",
            &client_key,
            self.settings.http_requests_per_minute_per_ip,
            Duration::from_secs(60),
        )
        .await
    }

    async fn enforce_distributed_rate_limit(
        &self,
        scope: &str,
        identity: &str,
        limit: u32,
        window: Duration,
    ) -> Result<(), GameError> {
        if limit == 0 {
            return Ok(());
        }
        let Some(mut connection) = self.distributed_event_publisher.clone() else {
            return Ok(());
        };
        let digest = hash_token(identity);
        let key = format!("mk01:rate-limit:{scope}:{digest}");
        let result: Result<i64, redis::RedisError> = redis::cmd("EVAL")
            .arg(DISTRIBUTED_RATE_LIMIT_SCRIPT)
            .arg(1)
            .arg(key)
            .arg(limit)
            .arg(window.as_millis().min(u128::from(u64::MAX)) as u64)
            .query_async(&mut connection)
            .await;
        match result {
            Ok(1) => {
                self.coordination_healthy.store(true, Ordering::Relaxed);
                Ok(())
            }
            Ok(_) => Err(GameError::RateLimited),
            Err(error) if self.settings.distributed_coordination_required => {
                self.coordination_healthy.store(false, Ordering::Relaxed);
                tracing::error!(%error, %scope, "required shared rate limiter unavailable");
                Err(GameError::StorageUnavailable)
            }
            Err(error) => {
                tracing::warn!(%error, %scope, "shared rate limiter unavailable; local limit retained");
                Ok(())
            }
        }
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

    pub async fn enforce_session_creation_rate_limit(
        &self,
        client_key: &str,
    ) -> Result<(), GameError> {
        self.session_creation_rate_limiter
            .check(client_key)
            .then_some(())
            .ok_or(GameError::RateLimited)?;
        self.enforce_distributed_rate_limit(
            "session-create-ip",
            client_key,
            self.settings.session_creations_per_minute,
            Duration::from_secs(60),
        )
        .await
    }

    pub async fn enforce_websocket_event_rate_limit(
        &self,
        session_id: Uuid,
    ) -> Result<(), GameError> {
        let key = session_id.to_string();
        self.websocket_event_rate_limiter
            .check(session_id.to_string())
            .then_some(())
            .ok_or(GameError::RateLimited)?;
        self.enforce_distributed_rate_limit(
            "websocket-session",
            &key,
            self.settings.websocket_events_per_second,
            Duration::from_secs(1),
        )
        .await
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
    let connection = tokio::time::timeout(DISTRIBUTED_EVENT_CONNECT_TIMEOUT, async {
        let client = redis::Client::open(settings.redis_url.as_str())?;
        let publisher = ConnectionManager::new(client.clone()).await?;
        let mut subscriber = client.get_async_pubsub().await?;
        subscriber.subscribe(DISTRIBUTED_EVENT_CHANNEL).await?;
        Ok::<_, redis::RedisError>((publisher, client, subscriber))
    })
    .await;
    match connection {
        Ok(Ok((publisher, client, subscriber))) => {
            Ok((Some(publisher), Some((client, subscriber))))
        }
        Ok(Err(error)) if settings.distributed_coordination_required => {
            tracing::error!(%error, "required distributed event coordination unavailable");
            Err(GameError::StorageUnavailable)
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, "distributed event coordination disabled; running single-instance only");
            Ok((None, None))
        }
        Err(_) if settings.distributed_coordination_required => {
            tracing::error!(
                timeout_ms = DISTRIBUTED_EVENT_CONNECT_TIMEOUT.as_millis(),
                "required distributed event coordination timed out"
            );
            Err(GameError::StorageUnavailable)
        }
        Err(_) => {
            tracing::warn!(
                timeout_ms = DISTRIBUTED_EVENT_CONNECT_TIMEOUT.as_millis(),
                "distributed event coordination timed out; running single-instance only"
            );
            Ok((None, None))
        }
    }
}

#[cfg(test)]
mod tests;
