use super::*;

#[derive(Debug)]
pub struct ServerMetrics {
    started_at: std::time::Instant,
    pub http_requests: AtomicU64,
    pub rate_limit_rejections: AtomicU64,
    pub websocket_connections: AtomicU64,
    pub websocket_events: AtomicU64,
    pub protocol_http_negotiations: [AtomicU64; PROTOCOL_VERSION_SLOT_COUNT],
    pub protocol_websocket_negotiations: [AtomicU64; PROTOCOL_VERSION_SLOT_COUNT],
    pub protocol_http_rejections: AtomicU64,
    pub protocol_websocket_rejections: AtomicU64,
    pub distributed_events_published: AtomicU64,
    pub distributed_event_failures: AtomicU64,
    pub room_mutations: AtomicU64,
    pub room_version_conflicts: AtomicU64,
    pub room_authority_acquisitions: AtomicU64,
    pub room_authority_conflicts: AtomicU64,
    pub matchmaking_queued: AtomicU64,
    pub ranked_matchmaking_queued: AtomicU64,
    pub matchmaking_completed: AtomicU64,
    pub ranked_matchmaking_completed: AtomicU64,
    pub ranked_matchmaking_rematches: AtomicU64,
    pub ranked_leaderboard_requests: AtomicU64,
    pub ranked_leaderboard_empty_responses: AtomicU64,
    pub ranked_leaderboard_entries_served: AtomicU64,
    pub ranked_leaderboard_visibility_changes: AtomicU64,
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
    pub live_content_published: AtomicU64,
    pub live_content_rollbacks: AtomicU64,
    http_responses_by_class: [AtomicU64; 5],
    command_latency: [[SloDistribution; 2]; CommandTransport::COUNT],
    matchmaking_latency: SloDistribution,
    active_match_recovery_latency: SloDistribution,
    pub websocket_disconnects: AtomicU64,
    pub unexpected_disconnects: AtomicU64,
    pub websocket_connected_milliseconds: AtomicU64,
    funnel_events: [[AtomicU64; FunnelOutcome::COUNT]; FunnelStage::COUNT],
    funnel_failures: [AtomicU64; FunnelFailureReason::COUNT],
    rum: [[[RumDistribution; RumDeviceTier::COUNT]; RumRoute::COUNT]; RumMetric::COUNT],
}

const RUM_BUCKET_COUNT: usize = 5;
const SLO_BUCKET_COUNT: usize = 6;
const PROTOCOL_VERSION_SLOT_COUNT: usize =
    (crate::MAX_SUPPORTED_PROTOCOL_VERSION - crate::MIN_SUPPORTED_PROTOCOL_VERSION + 1) as usize;
const COMMAND_LATENCY_BUCKETS_MS: [u64; SLO_BUCKET_COUNT] = [25, 50, 100, 150, 400, 1_000];
const MATCHMAKING_LATENCY_BUCKETS_SECONDS: [u64; SLO_BUCKET_COUNT] = [1, 5, 10, 30, 60, 120];
const RECOVERY_LATENCY_BUCKETS_MS: [u64; SLO_BUCKET_COUNT] =
    [1_000, 2_500, 5_000, 10_000, 30_000, 60_000];

fn protocol_metric_index(version: u16) -> Option<usize> {
    version
        .checked_sub(crate::MIN_SUPPORTED_PROTOCOL_VERSION)
        .map(usize::from)
        .filter(|index| *index < PROTOCOL_VERSION_SLOT_COUNT)
}

#[derive(Debug, Clone, Copy)]
pub enum CommandTransport {
    Http,
    Websocket,
}

impl CommandTransport {
    const COUNT: usize = 2;

    const fn index(self) -> usize {
        self as usize
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Websocket => "websocket",
        }
    }
}

#[derive(Debug, Default)]
struct SloDistribution {
    buckets: [AtomicU64; SLO_BUCKET_COUNT],
    count: AtomicU64,
    sum: AtomicU64,
}

impl SloDistribution {
    fn record(&self, value: u64, upper_bounds: &[u64; SLO_BUCKET_COUNT]) {
        for (index, upper_bound) in upper_bounds.iter().enumerate() {
            if value <= *upper_bound {
                self.buckets[index].fetch_add(1, Ordering::Relaxed);
            }
        }
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum.fetch_add(value, Ordering::Relaxed);
    }
}

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
            protocol_http_negotiations: std::array::from_fn(|_| AtomicU64::new(0)),
            protocol_websocket_negotiations: std::array::from_fn(|_| AtomicU64::new(0)),
            protocol_http_rejections: AtomicU64::new(0),
            protocol_websocket_rejections: AtomicU64::new(0),
            distributed_events_published: AtomicU64::new(0),
            distributed_event_failures: AtomicU64::new(0),
            room_mutations: AtomicU64::new(0),
            room_version_conflicts: AtomicU64::new(0),
            room_authority_acquisitions: AtomicU64::new(0),
            room_authority_conflicts: AtomicU64::new(0),
            matchmaking_queued: AtomicU64::new(0),
            ranked_matchmaking_queued: AtomicU64::new(0),
            matchmaking_completed: AtomicU64::new(0),
            ranked_matchmaking_completed: AtomicU64::new(0),
            ranked_matchmaking_rematches: AtomicU64::new(0),
            ranked_leaderboard_requests: AtomicU64::new(0),
            ranked_leaderboard_empty_responses: AtomicU64::new(0),
            ranked_leaderboard_entries_served: AtomicU64::new(0),
            ranked_leaderboard_visibility_changes: AtomicU64::new(0),
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
            live_content_published: AtomicU64::new(0),
            live_content_rollbacks: AtomicU64::new(0),
            http_responses_by_class: std::array::from_fn(|_| AtomicU64::new(0)),
            command_latency: std::array::from_fn(|_| {
                std::array::from_fn(|_| SloDistribution::default())
            }),
            matchmaking_latency: SloDistribution::default(),
            active_match_recovery_latency: SloDistribution::default(),
            websocket_disconnects: AtomicU64::new(0),
            unexpected_disconnects: AtomicU64::new(0),
            websocket_connected_milliseconds: AtomicU64::new(0),
            funnel_events: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU64::new(0))),
            funnel_failures: std::array::from_fn(|_| AtomicU64::new(0)),
            rum: std::array::from_fn(|_| {
                std::array::from_fn(|_| std::array::from_fn(|_| RumDistribution::default()))
            }),
        }
    }
}

impl ServerMetrics {
    pub fn record_protocol_http_negotiation(&self, version: u16) {
        if let Some(index) = protocol_metric_index(version) {
            self.protocol_http_negotiations[index].fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_protocol_websocket_negotiation(&self, version: u16) {
        if let Some(index) = protocol_metric_index(version) {
            self.protocol_websocket_negotiations[index].fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_http_response(&self, status: StatusCode) {
        let class = usize::from(status.as_u16() / 100);
        if (1..=5).contains(&class) {
            self.http_responses_by_class[class - 1].fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_command_latency(
        &self,
        transport: CommandTransport,
        accepted: bool,
        elapsed: Duration,
    ) {
        let outcome_index = usize::from(!accepted);
        self.command_latency[transport.index()][outcome_index].record(
            elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
            &COMMAND_LATENCY_BUCKETS_MS,
        );
    }

    pub fn record_matchmaking_latency(&self, queued_at: chrono::DateTime<Utc>) {
        let elapsed = Utc::now()
            .signed_duration_since(queued_at)
            .num_seconds()
            .max(0) as u64;
        self.matchmaking_latency
            .record(elapsed, &MATCHMAKING_LATENCY_BUCKETS_SECONDS);
    }

    pub fn record_websocket_disconnect(&self, elapsed: Duration, unexpected: bool) {
        self.websocket_disconnects.fetch_add(1, Ordering::Relaxed);
        self.websocket_connected_milliseconds.fetch_add(
            elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
        if unexpected {
            self.unexpected_disconnects.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_active_match_recovery(&self, elapsed: Duration) {
        self.active_match_recovery_latency.record(
            elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
            &RECOVERY_LATENCY_BUCKETS_MS,
        );
    }

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
                "mk01_ranked_matchmaking_queued_total",
                "Ranked matchmaking enqueue responses without an immediate match.",
                &self.ranked_matchmaking_queued,
            ),
            counter(
                "mk01_matchmaking_completed_total",
                "Durably completed matchmaking pairs.",
                &self.matchmaking_completed,
            ),
            counter(
                "mk01_ranked_matchmaking_completed_total",
                "Durably completed ranked matchmaking pairs.",
                &self.ranked_matchmaking_completed,
            ),
            counter(
                "mk01_ranked_matchmaking_rematches_total",
                "Durably completed ranked pairs that required recent-opponent relaxation.",
                &self.ranked_matchmaking_rematches,
            ),
            counter(
                "mk01_ranked_leaderboard_requests_total",
                "Successful ranked leaderboard page requests.",
                &self.ranked_leaderboard_requests,
            ),
            counter(
                "mk01_ranked_leaderboard_empty_responses_total",
                "Successful ranked leaderboard pages with no visible eligible entries.",
                &self.ranked_leaderboard_empty_responses,
            ),
            counter(
                "mk01_ranked_leaderboard_entries_served_total",
                "Privacy-filtered ranked leaderboard entries served.",
                &self.ranked_leaderboard_entries_served,
            ),
            counter(
                "mk01_ranked_leaderboard_visibility_changes_total",
                "Authenticated ranked leaderboard visibility preference changes.",
                &self.ranked_leaderboard_visibility_changes,
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
            counter(
                "mk01_live_content_published_total",
                "Validated live-content revisions published by operators.",
                &self.live_content_published,
            ),
            counter(
                "mk01_live_content_rollbacks_total",
                "Live-content rollback revisions published by operators.",
                &self.live_content_rollbacks,
            ),
            counter(
                "mk01_websocket_disconnects_total",
                "Completed WebSocket connections, regardless of close reason.",
                &self.websocket_disconnects,
            ),
            counter(
                "mk01_unexpected_disconnects_total",
                "WebSocket connections that ended without a normal client close frame.",
                &self.unexpected_disconnects,
            ),
            counter(
                "mk01_websocket_connected_milliseconds_total",
                "Cumulative connected-player WebSocket time in milliseconds.",
                &self.websocket_connected_milliseconds,
            ),
            gauge(
                "mk01_matchmaking_queue_depth",
                "Current durable matchmaking queue entries.",
                matchmaking.queued,
            ),
            gauge(
                "mk01_ranked_matchmaking_queue_depth",
                "Current durable ranked matchmaking queue entries.",
                matchmaking.ranked_queued,
            ),
            gauge(
                "mk01_matchmaking_oldest_age_seconds",
                "Age of the oldest durable matchmaking queue entry in seconds.",
                matchmaking.oldest_age_seconds,
            ),
        ]
        .concat();
        output.push_str(
            "# HELP mk01_protocol_negotiations_total Bounded client protocol negotiations by transport, version, and outcome.\n\
# TYPE mk01_protocol_negotiations_total counter\n",
        );
        for version in crate::MIN_SUPPORTED_PROTOCOL_VERSION..=crate::MAX_SUPPORTED_PROTOCOL_VERSION
        {
            let index = protocol_metric_index(version)
                .expect("the configured protocol range must have a bounded metric slot");
            for (transport, value) in [
                ("http", &self.protocol_http_negotiations[index]),
                ("websocket", &self.protocol_websocket_negotiations[index]),
            ] {
                output.push_str(&format!(
                    "mk01_protocol_negotiations_total{{transport=\"{transport}\",version=\"{version}\",outcome=\"accepted\"}} {}\n",
                    value.load(Ordering::Relaxed)
                ));
            }
        }
        for (transport, value) in [
            ("http", &self.protocol_http_rejections),
            ("websocket", &self.protocol_websocket_rejections),
        ] {
            output.push_str(&format!(
                "mk01_protocol_negotiations_total{{transport=\"{transport}\",version=\"unsupported\",outcome=\"rejected\"}} {}\n",
                value.load(Ordering::Relaxed)
            ));
        }
        output.push_str(
            "# HELP mk01_http_responses_total Product API responses by status class, excluding operational and telemetry routes.\n\
# TYPE mk01_http_responses_total counter\n",
        );
        for (index, value) in self.http_responses_by_class.iter().enumerate() {
            output.push_str(&format!(
                "mk01_http_responses_total{{class=\"{}xx\"}} {}\n",
                index + 1,
                value.load(Ordering::Relaxed)
            ));
        }
        for transport in [CommandTransport::Http, CommandTransport::Websocket] {
            for (outcome_index, outcome) in ["accepted", "rejected"].into_iter().enumerate() {
                render_slo_histogram(
                    &mut output,
                    "mk01_command_duration_milliseconds",
                    "Product command processing duration in milliseconds.",
                    &format!("transport=\"{}\",outcome=\"{outcome}\"", transport.label()),
                    &COMMAND_LATENCY_BUCKETS_MS,
                    &self.command_latency[transport.index()][outcome_index],
                    transport.index() == 0 && outcome_index == 0,
                );
            }
        }
        render_slo_histogram(
            &mut output,
            "mk01_matchmaking_duration_seconds",
            "Queue entry to durable room assignment duration for each matched player in seconds.",
            "",
            &MATCHMAKING_LATENCY_BUCKETS_SECONDS,
            &self.matchmaking_latency,
            true,
        );
        render_slo_histogram(
            &mut output,
            "mk01_active_match_recovery_milliseconds",
            "Persisted disconnect to authoritative active-match reconnection in milliseconds.",
            "",
            &RECOVERY_LATENCY_BUCKETS_MS,
            &self.active_match_recovery_latency,
            true,
        );
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

fn render_slo_histogram(
    output: &mut String,
    name: &str,
    help: &str,
    labels: &str,
    upper_bounds: &[u64; SLO_BUCKET_COUNT],
    distribution: &SloDistribution,
    include_metadata: bool,
) {
    if include_metadata {
        output.push_str(&format!("# HELP {name} {help}\n# TYPE {name} histogram\n"));
    }
    let label_prefix = if labels.is_empty() {
        String::new()
    } else {
        format!("{labels},")
    };
    for (index, upper_bound) in upper_bounds.iter().enumerate() {
        output.push_str(&format!(
            "{name}_bucket{{{label_prefix}le=\"{upper_bound}\"}} {}\n",
            distribution.buckets[index].load(Ordering::Relaxed)
        ));
    }
    let count = distribution.count.load(Ordering::Relaxed);
    let exact_labels = if labels.is_empty() {
        String::new()
    } else {
        format!("{{{labels}}}")
    };
    output.push_str(&format!(
        "{name}_bucket{{{label_prefix}le=\"+Inf\"}} {count}\n{name}_sum{exact_labels} {}\n{name}_count{exact_labels} {count}\n",
        distribution.sum.load(Ordering::Relaxed)
    ));
}
