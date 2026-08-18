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
    http::{HeaderValue, Method},
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
        IntegritySignalKind, IntegritySignalPage, MissionCadence, MissionProgress,
        ModerationAction, ModerationActionKind, ModerationCasePage, NewIntegritySignal,
        NewModerationAction, NewPlayerReport, Orientation, PlayerAccount, PlayerKind,
        PlayerProgression, PlayerReportReceipt, ReportCategory, ReportStatus, RoomVisibility,
        ShipKind, ShipPlacement, SocialRelationship, UserSession,
    },
    error::GameError,
    protocol::{
        CreateRoomInput, FunnelFailureReason, FunnelOutcome, FunnelStage, ProtocolError,
        RumDeviceTier, RumMetric, RumRoute, ServerEvent,
    },
    rate_limit::FixedWindowRateLimiter,
    store::{
        AccountDeletionScope, AccountDeletionStats, GameHistoryItem, GameStore,
        MatchmakingQueueStats, MemoryStore, MissionReward, PostgresRedisStore,
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

#[derive(Debug)]
pub struct ServerMetrics {
    started_at: std::time::Instant,
    pub http_requests: AtomicU64,
    pub rate_limit_rejections: AtomicU64,
    pub websocket_connections: AtomicU64,
    pub websocket_events: AtomicU64,
    pub distributed_events_published: AtomicU64,
    pub distributed_event_failures: AtomicU64,
    pub room_mutations: AtomicU64,
    pub room_version_conflicts: AtomicU64,
    pub room_authority_acquisitions: AtomicU64,
    pub room_authority_conflicts: AtomicU64,
    pub matchmaking_queued: AtomicU64,
    pub matchmaking_completed: AtomicU64,
    pub matchmaking_cancelled: AtomicU64,
    pub retention_sessions_deleted: AtomicU64,
    pub retention_rooms_deleted: AtomicU64,
    pub retention_matchmaking_deleted: AtomicU64,
    pub retention_moderation_deleted: AtomicU64,
    pub retention_integrity_deleted: AtomicU64,
    pub integrity_impossible_order: AtomicU64,
    pub integrity_automation: AtomicU64,
    pub integrity_collusion: AtomicU64,
    pub integrity_stalling: AtomicU64,
    funnel_events: [[AtomicU64; FunnelOutcome::COUNT]; FunnelStage::COUNT],
    funnel_failures: [AtomicU64; FunnelFailureReason::COUNT],
    rum: [[[RumDistribution; RumDeviceTier::COUNT]; RumRoute::COUNT]; RumMetric::COUNT],
}

const RUM_BUCKET_COUNT: usize = 5;

#[derive(Debug, Default)]
struct RumDistribution {
    buckets: [AtomicU64; RUM_BUCKET_COUNT],
    count: AtomicU64,
    sum: AtomicU64,
}

impl Default for ServerMetrics {
    fn default() -> Self {
        Self {
            started_at: std::time::Instant::now(),
            http_requests: AtomicU64::new(0),
            rate_limit_rejections: AtomicU64::new(0),
            websocket_connections: AtomicU64::new(0),
            websocket_events: AtomicU64::new(0),
            distributed_events_published: AtomicU64::new(0),
            distributed_event_failures: AtomicU64::new(0),
            room_mutations: AtomicU64::new(0),
            room_version_conflicts: AtomicU64::new(0),
            room_authority_acquisitions: AtomicU64::new(0),
            room_authority_conflicts: AtomicU64::new(0),
            matchmaking_queued: AtomicU64::new(0),
            matchmaking_completed: AtomicU64::new(0),
            matchmaking_cancelled: AtomicU64::new(0),
            retention_sessions_deleted: AtomicU64::new(0),
            retention_rooms_deleted: AtomicU64::new(0),
            retention_matchmaking_deleted: AtomicU64::new(0),
            retention_moderation_deleted: AtomicU64::new(0),
            retention_integrity_deleted: AtomicU64::new(0),
            integrity_impossible_order: AtomicU64::new(0),
            integrity_automation: AtomicU64::new(0),
            integrity_collusion: AtomicU64::new(0),
            integrity_stalling: AtomicU64::new(0),
            funnel_events: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU64::new(0))),
            funnel_failures: std::array::from_fn(|_| AtomicU64::new(0)),
            rum: std::array::from_fn(|_| {
                std::array::from_fn(|_| std::array::from_fn(|_| RumDistribution::default()))
            }),
        }
    }
}

impl ServerMetrics {
    pub fn record_funnel_event(
        &self,
        stage: FunnelStage,
        outcome: FunnelOutcome,
        reason: Option<FunnelFailureReason>,
    ) {
        self.funnel_events[stage.index()][outcome.index()].fetch_add(1, Ordering::Relaxed);
        if let Some(reason) = reason {
            self.funnel_failures[reason.index()].fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_rum_metric(
        &self,
        metric: RumMetric,
        route: RumRoute,
        device_tier: RumDeviceTier,
        value: u32,
    ) {
        let distribution = &self.rum[metric.index()][route.index()][device_tier.index()];
        for (index, upper_bound) in metric.buckets().into_iter().enumerate() {
            if u64::from(value) <= upper_bound {
                distribution.buckets[index].fetch_add(1, Ordering::Relaxed);
            }
        }
        distribution.count.fetch_add(1, Ordering::Relaxed);
        distribution
            .sum
            .fetch_add(u64::from(value), Ordering::Relaxed);
    }

    pub fn render_prometheus(&self, matchmaking: MatchmakingQueueStats) -> String {
        let counter = |name: &str, help: &str, value: &AtomicU64| {
            format!(
                "# HELP {name} {help}\n# TYPE {name} counter\n{name} {}\n",
                value.load(Ordering::Relaxed)
            )
        };
        let gauge = |name: &str, help: &str, value: u64| {
            format!("# HELP {name} {help}\n# TYPE {name} gauge\n{name} {value}\n")
        };
        let mut output = [
            gauge(
                "mk01_process_uptime_seconds",
                "Process uptime in seconds.",
                self.started_at.elapsed().as_secs(),
            ),
            counter(
                "mk01_http_requests_total",
                "HTTP and WebSocket upgrade requests received.",
                &self.http_requests,
            ),
            counter(
                "mk01_rate_limit_rejections_total",
                "Requests rejected by an application or shared rate limit.",
                &self.rate_limit_rejections,
            ),
            gauge(
                "mk01_websocket_connections",
                "Current WebSocket connections on this instance.",
                self.websocket_connections.load(Ordering::Relaxed),
            ),
            counter(
                "mk01_websocket_events_total",
                "Accepted inbound WebSocket events.",
                &self.websocket_events,
            ),
            counter(
                "mk01_distributed_events_published_total",
                "Events published to the cross-instance channel.",
                &self.distributed_events_published,
            ),
            counter(
                "mk01_distributed_event_failures_total",
                "Cross-instance event publish failures.",
                &self.distributed_event_failures,
            ),
            counter(
                "mk01_room_mutations_total",
                "Successfully persisted room mutations.",
                &self.room_mutations,
            ),
            counter(
                "mk01_room_version_conflicts_total",
                "Rejected stale room persistence revisions.",
                &self.room_version_conflicts,
            ),
            counter(
                "mk01_room_authority_acquisitions_total",
                "Successfully acquired room mutation authority leases.",
                &self.room_authority_acquisitions,
            ),
            counter(
                "mk01_room_authority_conflicts_total",
                "Room mutations rejected because another authority lease was active.",
                &self.room_authority_conflicts,
            ),
            counter(
                "mk01_matchmaking_queued_total",
                "Matchmaking enqueue responses without an immediate match.",
                &self.matchmaking_queued,
            ),
            counter(
                "mk01_matchmaking_completed_total",
                "Durably completed matchmaking pairs.",
                &self.matchmaking_completed,
            ),
            counter(
                "mk01_matchmaking_cancelled_total",
                "Successfully cancelled matchmaking entries.",
                &self.matchmaking_cancelled,
            ),
            counter(
                "mk01_retention_sessions_deleted_total",
                "Expired inactive sessions removed by retention sweeps.",
                &self.retention_sessions_deleted,
            ),
            counter(
                "mk01_retention_rooms_deleted_total",
                "Expired completed rooms removed by retention sweeps.",
                &self.retention_rooms_deleted,
            ),
            counter(
                "mk01_retention_matchmaking_deleted_total",
                "Abandoned matchmaking entries removed by retention sweeps.",
                &self.retention_matchmaking_deleted,
            ),
            counter(
                "mk01_retention_moderation_deleted_total",
                "Closed moderation cases removed by retention sweeps.",
                &self.retention_moderation_deleted,
            ),
            counter(
                "mk01_retention_integrity_deleted_total",
                "Expired game-integrity signals removed by retention sweeps.",
                &self.retention_integrity_deleted,
            ),
            counter(
                "mk01_integrity_impossible_order_total",
                "Impossible or out-of-order authoritative commands detected.",
                &self.integrity_impossible_order,
            ),
            counter(
                "mk01_integrity_automation_total",
                "Automation-like event bursts detected.",
                &self.integrity_automation,
            ),
            counter(
                "mk01_integrity_collusion_total",
                "Repeated suspicious short-match pairings detected.",
                &self.integrity_collusion,
            ),
            counter(
                "mk01_integrity_stalling_total",
                "Repeated authoritative turn timeouts detected.",
                &self.integrity_stalling,
            ),
            gauge(
                "mk01_matchmaking_queue_depth",
                "Current durable matchmaking queue entries.",
                matchmaking.queued,
            ),
            gauge(
                "mk01_matchmaking_oldest_age_seconds",
                "Age of the oldest durable matchmaking queue entry in seconds.",
                matchmaking.oldest_age_seconds,
            ),
        ]
        .concat();
        output.push_str(
            "# HELP mk01_new_player_funnel_events_total Aggregate onboarding events by fixed stage and outcome.\n\
# TYPE mk01_new_player_funnel_events_total counter\n",
        );
        for stage in FunnelStage::ALL {
            for outcome in FunnelOutcome::ALL {
                output.push_str(&format!(
                    "mk01_new_player_funnel_events_total{{stage=\"{}\",outcome=\"{}\"}} {}\n",
                    stage.label(),
                    outcome.label(),
                    self.funnel_events[stage.index()][outcome.index()].load(Ordering::Relaxed)
                ));
            }
        }
        output.push_str(
            "# HELP mk01_new_player_funnel_failures_total Aggregate onboarding failures by fixed reason.\n\
# TYPE mk01_new_player_funnel_failures_total counter\n",
        );
        for reason in FunnelFailureReason::ALL {
            output.push_str(&format!(
                "mk01_new_player_funnel_failures_total{{reason=\"{}\"}} {}\n",
                reason.label(),
                self.funnel_failures[reason.index()].load(Ordering::Relaxed)
            ));
        }
        for metric in RumMetric::ALL {
            let name = metric.prometheus_name();
            output.push_str(&format!(
                "# HELP {name} {}\n# TYPE {name} histogram\n",
                metric.help()
            ));
            for route in RumRoute::ALL {
                for device_tier in RumDeviceTier::ALL {
                    let distribution =
                        &self.rum[metric.index()][route.index()][device_tier.index()];
                    let count = distribution.count.load(Ordering::Relaxed);
                    if count == 0 {
                        continue;
                    }
                    for (index, upper_bound) in metric.buckets().into_iter().enumerate() {
                        output.push_str(&format!(
                            "{name}_bucket{{route=\"{}\",device_tier=\"{}\",le=\"{upper_bound}\"}} {}\n",
                            route.label(),
                            device_tier.label(),
                            distribution.buckets[index].load(Ordering::Relaxed)
                        ));
                    }
                    output.push_str(&format!(
                        "{name}_bucket{{route=\"{}\",device_tier=\"{}\",le=\"+Inf\"}} {count}\n\
{name}_sum{{route=\"{}\",device_tier=\"{}\"}} {}\n\
{name}_count{{route=\"{}\",device_tier=\"{}\"}} {count}\n",
                        route.label(),
                        device_tier.label(),
                        route.label(),
                        device_tier.label(),
                        distribution.sum.load(Ordering::Relaxed),
                        route.label(),
                        device_tier.label()
                    ));
                }
            }
        }
        output
    }
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

    pub async fn create_session(
        &self,
        nickname: String,
    ) -> Result<(UserSession, String), GameError> {
        let nickname = nickname.trim().to_string();
        validate_nickname(&nickname)?;
        let token = random_token();
        let token_hash = hash_token(&token);
        let now = Utc::now();
        let session = UserSession {
            id: Uuid::new_v4(),
            account_id: None,
            nickname,
            token_hash,
            created_at: now,
            last_seen_at: now,
            current_room_id: None,
        };
        self.store.save_session(&session).await?;
        Ok((session, token))
    }

    pub async fn upgrade_account(
        &self,
        session: &UserSession,
        handle: String,
    ) -> Result<(PlayerAccount, String, String), GameError> {
        if session.account_id.is_some() {
            return Err(GameError::InvalidState);
        }
        if session.current_room_id.is_some() {
            return Err(GameError::InvalidState);
        }
        let handle = handle.trim().to_string();
        validate_nickname(&handle)?;
        let recovery_key = random_token();
        let next_session_token = random_token();
        let account = PlayerAccount {
            id: Uuid::new_v4(),
            handle,
            created_at: Utc::now(),
        };
        self.store
            .create_account(
                session.id,
                &account,
                &hash_token(&recovery_key),
                &hash_token(&next_session_token),
            )
            .await?;
        Ok((account, recovery_key, next_session_token))
    }

    pub async fn login_account(
        &self,
        account_id: Uuid,
        recovery_key: String,
    ) -> Result<(UserSession, String), GameError> {
        if recovery_key.len() != 43
            || !recovery_key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(GameError::Unauthorized);
        }
        let account = self
            .store
            .account_by_credentials(account_id, &hash_token(&recovery_key))
            .await?
            .ok_or(GameError::Unauthorized)?;
        match self.store.active_penalty(account.id, account.id).await? {
            Some(ActivePenalty::Banned) => return Err(GameError::AccountBanned),
            Some(ActivePenalty::Suspended(_)) => return Err(GameError::AccountSuspended),
            None => {}
        }
        let token = random_token();
        let now = Utc::now();
        let session = UserSession {
            id: Uuid::new_v4(),
            account_id: Some(account.id),
            nickname: account.handle,
            token_hash: hash_token(&token),
            created_at: now,
            last_seen_at: now,
            current_room_id: None,
        };
        self.store.save_session(&session).await?;
        Ok((session, token))
    }

    pub async fn account_sessions(
        &self,
        session: &UserSession,
    ) -> Result<Vec<AccountSession>, GameError> {
        self.store
            .sessions_for_account(session.account_id.ok_or(GameError::Unauthorized)?)
            .await
    }

    pub async fn export_account_data(
        &self,
        session: &UserSession,
    ) -> Result<serde_json::Value, GameError> {
        let account_id = session.account_id.ok_or(GameError::Unauthorized)?;
        let request_id = Uuid::new_v4();
        let generated_at = Utc::now();
        let subject_fingerprint = hash_token(&format!("{account_id}:{request_id}"));
        self.store
            .export_account_data(account_id, request_id, &subject_fingerprint, generated_at)
            .await
    }

    pub async fn delete_account(
        &self,
        session: &UserSession,
        recovery_key: String,
        confirmation: String,
    ) -> Result<(Uuid, chrono::DateTime<Utc>, AccountDeletionStats), GameError> {
        let account_id = session.account_id.ok_or(GameError::Unauthorized)?;
        if confirmation != "DELETE"
            || recovery_key.len() != 43
            || !recovery_key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(GameError::InvalidRequest);
        }
        self.store
            .account_by_credentials(account_id, &hash_token(&recovery_key))
            .await?
            .ok_or(GameError::Unauthorized)?;

        let account_sessions = self.store.sessions_for_account(account_id).await?;
        let session_ids: HashSet<_> = account_sessions.iter().map(|item| item.id).collect();
        let mut known_room_ids: HashSet<_> = account_sessions
            .iter()
            .filter_map(|item| item.current_room_id)
            .collect();
        let cached_rooms: Vec<_> = self
            .rooms
            .iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect();
        for (room_id, room) in cached_rooms {
            if room
                .lock()
                .await
                .players
                .iter()
                .any(|player| session_ids.contains(&player.session_id))
            {
                known_room_ids.insert(room_id);
            }
        }

        for account_session in &account_sessions {
            let _ = self.cancel_matchmaking(account_session.id).await;
            if let Some(room_id) = account_session.current_room_id {
                let session_to_remove = UserSession {
                    id: account_session.id,
                    account_id: Some(account_id),
                    nickname: account_session.nickname.clone(),
                    token_hash: String::new(),
                    created_at: account_session.created_at,
                    last_seen_at: account_session.last_seen_at,
                    current_room_id: Some(room_id),
                };
                match self.leave_room(&session_to_remove, room_id).await {
                    Ok(room) => {
                        self.broadcast_snapshots(&room, SnapshotEvent::PlayerLeft)
                            .await;
                        self.broadcast_latest_chat_message(&room).await;
                    }
                    Err(GameError::RoomNotFound) => {
                        self.store
                            .update_session_room(account_session.id, None)
                            .await?;
                    }
                    Err(error) => return Err(error),
                }
            }
            self.close_session_everywhere(account_session.id, GameError::Unauthorized)
                .await;
        }

        let request_id = Uuid::new_v4();
        let deleted_at = Utc::now();
        let subject_fingerprint = hash_token(&format!("{account_id}:{request_id}"));
        let known_room_ids: Vec<_> = known_room_ids.into_iter().collect();
        let stats = self
            .store
            .delete_account_data(
                account_id,
                request_id,
                &subject_fingerprint,
                &known_room_ids,
                deleted_at,
                AccountDeletionScope::LiveRequest,
            )
            .await?;
        for room_id in known_room_ids {
            self.rooms.remove(&room_id);
            self.cancel_turn_expiry(room_id);
        }
        Ok((request_id, deleted_at, stats))
    }

    pub async fn progression(&self, session: &UserSession) -> Result<PlayerProgression, GameError> {
        let history = self.store.history_for_session(session.id).await?;
        let rewards = match session.account_id {
            Some(account_id) => self.store.mission_rewards(account_id).await?,
            None => Vec::new(),
        };
        Ok(build_progression(session, &history, &rewards, Utc::now()))
    }

    pub async fn claim_mission_reward(
        &self,
        session: &UserSession,
        mission_id: &str,
    ) -> Result<PlayerProgression, GameError> {
        let account_id = session.account_id.ok_or(GameError::Unauthorized)?;
        let history = self.store.history_for_session(session.id).await?;
        let rewards = self.store.mission_rewards(account_id).await?;
        let now = Utc::now();
        let progression = build_progression(session, &history, &rewards, now);
        let mission = progression
            .missions
            .iter()
            .find(|mission| mission.id == mission_id)
            .ok_or(GameError::InvalidRequest)?;
        if !mission.completed {
            return Err(GameError::InvalidState);
        }
        let period_key = mission_period_key(mission.cadence, now);
        self.store
            .claim_mission_reward(account_id, mission.id, &period_key, mission.reward_xp)
            .await?;
        let rewards = self.store.mission_rewards(account_id).await?;
        Ok(build_progression(session, &history, &rewards, now))
    }

    pub async fn social_relationships(
        &self,
        session: &UserSession,
    ) -> Result<Vec<SocialRelationship>, GameError> {
        self.store
            .social_relationships(session.account_id.unwrap_or(session.id))
            .await
    }

    pub async fn update_social_relationship(
        &self,
        session: &UserSession,
        room_id: Uuid,
        target_player_id: Uuid,
        muted: bool,
        blocked: bool,
    ) -> Result<SocialRelationship, GameError> {
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
        let relationship = SocialRelationship {
            target_identity_id,
            target_nickname: target.nickname.clone(),
            muted,
            blocked,
            updated_at: Utc::now(),
        };
        self.store
            .set_social_relationship(actor_identity_id, relationship.clone())
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

    async fn detect_finished_match_integrity(&self, room: &GameRoom) -> Result<(), GameError> {
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

    async fn reconcile_runtime_state(&self, room: &mut GameRoom) -> Result<(), GameError> {
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
            if self.save_room(&mut room).await.is_err() {
                return;
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

    async fn restore_active_rooms(&self) -> Result<(), GameError> {
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

    pub async fn enqueue_matchmaking(
        &self,
        session: UserSession,
    ) -> Result<Option<GameRoom>, GameError> {
        if session.current_room_id.is_some() {
            return Err(GameError::AlreadyJoined);
        }
        if self.store.matchmaking_time(session.id).await?.is_none()
            && self.store.matchmaking_queue_stats().await?.queued
                >= self.settings.max_matchmaking_queue
        {
            return Err(GameError::CapacityReached);
        }
        let queued = self.store.enqueue_matchmaking(&session).await?;
        let Some(claim) = queued.claim else {
            self.metrics
                .matchmaking_queued
                .fetch_add(1, Ordering::Relaxed);
            return Ok(None);
        };
        let claim_id = claim.id;
        let result = async {
            let code = self.unique_room_code().await?;
            let mut room = GameRoom::new(
                code,
                "신속 교전".to_string(),
                RoomVisibility::Private,
                &claim.opponent,
            )?;
            room.join(&session)?;
            self.store.complete_matchmaking(claim_id, &mut room).await?;
            self.metrics
                .matchmaking_completed
                .fetch_add(1, Ordering::Relaxed);
            self.rooms
                .insert(room.id, Arc::new(Mutex::new(room.clone())));
            self.broadcast_snapshots(&room, SnapshotEvent::PlayerJoined)
                .await;
            self.broadcast_latest_chat_message(&room).await;
            Ok::<_, GameError>(room)
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
        result.map(Some)
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

    pub async fn matchmaking_time(
        &self,
        session_id: Uuid,
    ) -> Result<Option<chrono::DateTime<Utc>>, GameError> {
        self.store.matchmaking_time(session_id).await
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

fn build_progression(
    session: &UserSession,
    history: &[GameHistoryItem],
    rewards: &[MissionReward],
    now: chrono::DateTime<Utc>,
) -> PlayerProgression {
    let mut wins = 0_u32;
    let mut shots = 0_u32;
    let mut hits = 0_u32;
    let mut ships_sunk = 0_u32;
    let mut daily_games = 0_u32;
    let mut daily_hits = 0_u32;
    let mut weekly_wins = 0_u32;
    for item in history {
        let won = item.result.winner_id == item.self_player_id;
        wins += u32::from(won);
        let player = item
            .result
            .players
            .iter()
            .find(|player| player.player_id == item.self_player_id);
        if let Some(player) = player {
            shots = shots.saturating_add(player.shots);
            hits = hits.saturating_add(player.hits);
            ships_sunk = ships_sunk.saturating_add(u32::from(player.ships_sunk));
        }
        let finished_date = item.result.finished_at.date_naive();
        if finished_date == now.date_naive() {
            daily_games += 1;
            daily_hits = daily_hits.saturating_add(player.map_or(0, |stats| stats.hits));
        }
        if finished_date.iso_week() == now.date_naive().iso_week() {
            weekly_wins += u32::from(won);
        }
    }
    let games_played = u32::try_from(history.len()).unwrap_or(u32::MAX);
    let losses = games_played.saturating_sub(wins);
    // Progression is a deterministic projection of the authoritative result ledger. Re-saving a
    // result cannot double-award XP, and correcting/removing a result automatically rolls it back.
    let result_xp = u64::from(games_played) * 100
        + u64::from(wins) * 100
        + u64::from(hits) * 3
        + u64::from(ships_sunk) * 15;
    let total_xp = result_xp.saturating_add(
        rewards
            .iter()
            .map(|reward| u64::from(reward.xp))
            .sum::<u64>(),
    );
    const XP_PER_LEVEL: u64 = 500;
    let level = (total_xp / XP_PER_LEVEL + 1).min(100) as u32;
    let level_xp = if level == 100 {
        XP_PER_LEVEL
    } else {
        total_xp % XP_PER_LEVEL
    };
    let xp_to_next_level = if level == 100 {
        0
    } else {
        XP_PER_LEVEL - level_xp
    };
    let rank_title = match level {
        1..=4 => "CADET",
        5..=14 => "LIEUTENANT",
        15..=29 => "COMMANDER",
        30..=49 => "CAPTAIN",
        50..=74 => "COMMODORE",
        _ => "ADMIRAL",
    }
    .to_string();
    let accuracy_percent = if shots == 0 {
        0
    } else {
        ((u64::from(hits) * 100) / u64::from(shots)) as u32
    };
    let achievement =
        |id, title, description, progress: u32, target: u32, unlocked: bool| AchievementProgress {
            id,
            title,
            description,
            progress,
            target,
            unlocked,
        };
    let mission = |id: &'static str,
                   cadence,
                   title: &'static str,
                   description: &'static str,
                   progress: u32,
                   target: u32,
                   reward_xp: u32| {
        let period_key = mission_period_key(cadence, now);
        let claimed = rewards
            .iter()
            .any(|reward| reward.mission_id == id && reward.period_key == period_key);
        MissionProgress {
            id,
            cadence,
            title,
            description,
            progress,
            target,
            reward_xp,
            completed: progress >= target,
            claimed,
            claimable: session.account_id.is_some() && progress >= target && !claimed,
        }
    };
    PlayerProgression {
        account_id: session.account_id,
        handle: session.nickname.clone(),
        level,
        rank_title,
        total_xp,
        level_xp,
        xp_to_next_level,
        games_played,
        wins,
        losses,
        total_shots: shots,
        total_hits: hits,
        total_ships_sunk: ships_sunk,
        achievements: vec![
            achievement(
                "FIRST_CONTACT",
                "첫 접촉",
                "첫 번째 교전을 완료했습니다.",
                games_played,
                1,
                games_played >= 1,
            ),
            achievement(
                "FIRST_VICTORY",
                "첫 승전보",
                "첫 번째 승리를 기록했습니다.",
                wins,
                1,
                wins >= 1,
            ),
            achievement(
                "FLEET_BREAKER",
                "함대 파쇄자",
                "적 함선 25척을 격침했습니다.",
                ships_sunk,
                25,
                ships_sunk >= 25,
            ),
            achievement(
                "SHARPSHOOTER",
                "명사수",
                "20발 이상 사격하고 누적 명중률 60%를 달성했습니다.",
                accuracy_percent,
                60,
                shots >= 20 && accuracy_percent >= 60,
            ),
            achievement(
                "VETERAN",
                "베테랑 지휘관",
                "교전 25회를 완료했습니다.",
                games_played,
                25,
                games_played >= 25,
            ),
        ],
        missions: vec![
            mission(
                "DAILY_DEPLOYMENT",
                MissionCadence::Daily,
                "오늘의 출항",
                "오늘 교전 1회를 완료하십시오.",
                daily_games,
                1,
                100,
            ),
            mission(
                "DAILY_ACCURACY",
                MissionCadence::Daily,
                "정밀 포격",
                "오늘 적 함선 칸 10개를 명중시키십시오.",
                daily_hits,
                10,
                150,
            ),
            mission(
                "WEEKLY_SUPREMACY",
                MissionCadence::Weekly,
                "주간 제해권",
                "이번 주 교전 3회에서 승리하십시오.",
                weekly_wins,
                3,
                400,
            ),
        ],
        calculated_at: now,
    }
}

fn mission_period_key(cadence: MissionCadence, now: chrono::DateTime<Utc>) -> String {
    match cadence {
        MissionCadence::Daily => now.format("%Y-%m-%d").to_string(),
        MissionCadence::Weekly => {
            let week = now.date_naive().iso_week();
            format!("{}-W{:02}", week.year(), week.week())
        }
    }
}

fn practice_fleet() -> Vec<ShipPlacement> {
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

fn select_ai_coordinate(room: &GameRoom, ai_player_id: Uuid) -> Option<Coordinate> {
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
        Ok((publisher, client, subscriber)) => Ok((Some(publisher), Some((client, subscriber)))),
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

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
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
    state.metrics.http_requests.fetch_add(1, Ordering::Relaxed);
    if let Err(error) = state
        .enforce_ip_rate_limit(request.headers(), address)
        .await
    {
        if error == GameError::RateLimited {
            state
                .metrics
                .rate_limit_rejections
                .fetch_add(1, Ordering::Relaxed);
        }
        return Err(error);
    }
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
            account_id: None,
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

    #[test]
    fn progression_is_a_deterministic_projection_of_results() {
        use crate::domain::{FinishReason, GameResult, PlayerStatistics, WinType};

        let commander = session("Commander");
        let player_id = Uuid::new_v4();
        let now = Utc::now();
        let history = vec![GameHistoryItem {
            room_id: Uuid::new_v4(),
            room_name: "Progression test".to_string(),
            self_player_id: player_id,
            result: GameResult {
                winner_id: player_id,
                loser_id: Uuid::new_v4(),
                total_turns: 20,
                duration_seconds: 300,
                finished_at: now,
                players: vec![PlayerStatistics {
                    player_id,
                    shots: 20,
                    hits: 12,
                    ships_sunk: 5,
                    accuracy: 0.6,
                    total_timeouts: 0,
                }],
                finish_reason: FinishReason::FleetDestroyed,
                win_type: WinType::NormalVictory,
            },
        }];
        let first = build_progression(&commander, &history, &[], now);
        let repeated = build_progression(&commander, &history, &[], now);
        assert_eq!(first.total_xp, 311);
        assert_eq!(first.total_xp, repeated.total_xp);
        assert_eq!(first.games_played, 1);
        assert_eq!(first.wins, 1);
        assert!(first.achievements[0].unlocked);
        assert!(first.achievements[1].unlocked);
        assert!(first.achievements[3].unlocked);
        assert!(first.missions[0].completed);
        assert!(first.missions[1].completed);
    }

    #[test]
    fn ai_targeting_is_deterministic_and_never_repeats_a_resolved_cell() {
        let human = session("Trainee");
        let ai = session("MK-AI OFFICER");
        let mut room = GameRoom::new(
            "AI2345".to_string(),
            "AI training".to_string(),
            RoomVisibility::Private,
            &human,
        )
        .unwrap();
        room.join(&ai).unwrap();
        room.configure_practice(human.id, ai.id, AiDifficulty::Officer, practice_fleet())
            .unwrap();
        room.place_ships(human.id, fleet(0)).unwrap();
        room.confirm_placement(human.id, &fleet(0), 60).unwrap();

        let ai_player = room.player_for_session(ai.id).unwrap().clone();
        room.game.as_mut().unwrap().current_player_id = ai_player.id;
        let first = select_ai_coordinate(&room, ai_player.id).unwrap();
        assert_eq!(select_ai_coordinate(&room, ai_player.id), Some(first));
        let version = room.version;
        let turn = room.game.as_ref().unwrap().turn_number;
        room.fire(ai.id, Uuid::new_v4(), ai_player.id, first, version, turn)
            .unwrap();
        assert_ne!(select_ai_coordinate(&room, ai_player.id), Some(first));
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
        state.schedule_turn_expiry(room.timer_state(Utc::now()));

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

    #[tokio::test]
    async fn finished_match_assessment_detects_repeated_short_pairing_and_stalling() {
        let first = session("Integrity Alpha");
        let second = session("Integrity Bravo");
        let store = Arc::new(MemoryStore::default());
        store.save_session(&first).await.unwrap();
        store.save_session(&second).await.unwrap();
        let state = AppState::with_store(Settings::default(), store.clone());

        for index in 0..3 {
            let mut room = GameRoom::new(
                format!("INT{index}23"),
                format!("Integrity match {index}"),
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
            room.confirm_placement(first.id, &fleet(0), 60).unwrap();
            room.confirm_placement(second.id, &fleet(5), 60).unwrap();
            if index == 2 {
                room.game
                    .as_mut()
                    .unwrap()
                    .total_timeout_counts
                    .insert(first_player_id, 3);
            }
            room.surrender(second.id, second_player_id).unwrap();
            state.save_room(&mut room).await.unwrap();
        }

        let collusion = store
            .integrity_signals(None, Some(IntegritySignalKind::Collusion), None, 25)
            .await
            .unwrap();
        assert_eq!(collusion.signals.len(), 2);
        assert!(
            collusion
                .signals
                .iter()
                .all(|signal| signal.evidence["suspiciousShortMatchesSevenDays"] == 3)
        );
        let stalling = store
            .integrity_signals(
                None,
                Some(IntegritySignalKind::IntentionalStalling),
                None,
                25,
            )
            .await
            .unwrap();
        assert_eq!(stalling.signals.len(), 1);
        assert_eq!(stalling.signals[0].evidence["totalTimeouts"], 3);
    }
}
