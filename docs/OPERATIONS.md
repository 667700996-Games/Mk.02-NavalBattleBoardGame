# Mk.01 Service Operations Baseline

This runbook defines the first production service-level gates for Mk.01. It is intentionally
executable: a release is not production-ready until the staging drills below have produced dated
evidence owned by the release team.

## Initial SLOs

| Signal                  | Objective                             | Measurement                                                                  |
| ----------------------- | ------------------------------------- | ---------------------------------------------------------------------------- |
| API availability        | 99.95% per rolling 30 days            | non-5xx product API responses; operational/telemetry routes are excluded     |
| Command latency         | p95 < 150 ms, p99 < 400 ms            | authenticated HTTP and accepted WebSocket command spans                      |
| Matchmaking latency     | p95 < 30 s in a healthy regional pool | enqueue to durable room assignment                                           |
| Unexpected disconnects  | < 0.5% of completed socket sessions   | abnormal socket closes / all completed sockets; connected time is exposure   |
| Active-match recovery   | p95 < 10 s after instance loss        | last accepted room revision to first recovered snapshot                      |
| Persistence correctness | zero stale commits                    | `mk01_room_version_conflicts_total` may rise; stale commits must remain zero |

The first two 30-day windows are calibration windows. Any objective change requires a dated ADR;
silently relaxing an alert is not allowed.

## Metrics and minimum alerts

The server exposes Prometheus text at `/api/metrics`. The endpoint currently includes request and
rate-limit totals, local WebSocket connections/events, distributed publish success/failure, room
mutation/version-conflict and authority acquisition/conflict totals, matchmaking
queue/completion/cancellation totals, ranked queue/completion totals, total and ranked queue depth,
oldest queue-entry age, and retention
deletion totals for sessions, completed rooms, abandoned queue entries, closed moderation cases,
integrity signals, the privacy-preserving new-player funnel, and bounded real-user performance
histograms below.

### Core service SLO signals

The SLO numerator and denominator are emitted by the same process boundary so health probes and
client-side telemetry cannot silently dilute product availability or command latency:

- `mk01_http_responses_total{class}` counts product API status classes and excludes health,
  readiness, metrics, and anonymous telemetry routes.
  Thirty-day availability is
  `sum(increase(mk01_http_responses_total{class!="5xx"}[30d])) / sum(increase(mk01_http_responses_total[30d]))`.
  Require traffic in the denominator. Page when error-budget burn is true for either the one-hour
  + five-minute pair at 14.4× or the six-hour + thirty-minute pair at 6×.
- `mk01_command_duration_milliseconds{transport,outcome}` uses identical fixed buckets for
  authenticated HTTP product routes and every parsed WebSocket command. Public session creation,
  login, health, readiness, metrics and anonymous telemetry routes are excluded. The release p95
  query is
  `histogram_quantile(0.95, sum by (le) (rate(mk01_command_duration_milliseconds_bucket{outcome="accepted"}[5m])))`;
  use `0.99` for p99 and show HTTP/WebSocket splits beside the combined panel.
- `mk01_matchmaking_duration_seconds` records two observations per durable pair: each player's
  original queue time to the successful room transaction. The p95 query is
  `histogram_quantile(0.95, sum by (le) (rate(mk01_matchmaking_duration_seconds_bucket[15m])))`.
  Compare `mk01_ranked_matchmaking_queue_depth` with `mk01_matchmaking_queue_depth`, and alert on
  ranked completion starvation when ranked queue depth is non-zero but
  `increase(mk01_ranked_matchmaking_completed_total[15m])` remains zero. Search phase and widening
  limits are defined in `RANKED_MATCHMAKING.md`; do not expand them ad hoc during an incident.
- `mk01_websocket_disconnects_total`, `mk01_unexpected_disconnects_total`, and
  `mk01_websocket_connected_milliseconds_total` distinguish a normal client Close frame from an
  abnormal EOF or send/receive failure. The primary rate is
  `sum(increase(mk01_unexpected_disconnects_total[30d])) / sum(increase(mk01_websocket_disconnects_total[30d]))`.
  Also graph abnormal disconnects per connected-player hour by dividing the numerator by
  `sum(increase(mk01_websocket_connected_milliseconds_total[30d])) / 3600000`; require at least
  100 completed sockets before enforcing the percentage objective.
- `mk01_active_match_recovery_milliseconds` starts at the persisted disconnect time reconstructed
  from the reconnect deadline and ends only after the replacement authority saves the reconnected
  room. Its p95 query is
  `histogram_quantile(0.95, sum by (le) (rate(mk01_active_match_recovery_milliseconds_bucket[15m])))`.

All histograms are cumulative Prometheus histograms with bounded, identity-free labels. A release
must show non-zero accepted-command samples and, in the staging failure drill, non-zero recovery
samples. Missing series are a failed gate, not a zero-latency result.

### New-player funnel

`mk01_new_player_funnel_events_total{stage,outcome}` records the ordered checkpoints
`landing`, `tutorial_started`, `tutorial_completed`, `session_created`, `lobby_entered`,
`room_joined`, `placement_completed`, `first_attack`, and `match_completed`. `outcome` is one of
`reached`, `failed`, or `abandoned`. `mk01_new_player_funnel_failures_total{reason}` splits failures
into the fixed reasons `network`, `session_creation`, `authentication`, `room_entry`,
`matchmaking`, `recovery`, `placement`, and `attack`.

These are event counters, not player or account counts. A browser tab emits each reached checkpoint
once per session-storage lifetime. The landing checkpoint excludes an already authenticated return
visit; direct invitations can therefore begin at `session_created`. An explicit tutorial exit,
matchmaking cancellation, pre-result room leave, or browser unload/reload before completion counts
as abandonment at the furthest reached checkpoint. A later resumed checkpoint remains visible, so
operators can distinguish interrupted-and-recovered flows from permanent aggregate loss. The
ingest schema rejects arbitrary labels, unknown fields, player/session IDs, and failed events that
lack a fixed reason. The global per-IP request limit and 64 KiB body limit protect the anonymous
endpoint; funnel data must never be used as authoritative billing, ranking, or anti-cheat evidence.

The minimum dashboard has four ordered panels over both 24-hour and 7-day windows:

1. reached volume by stage:
   `sum by (stage) (increase(mk01_new_player_funnel_events_total{outcome="reached"}[24h]))`;
2. abandonment volume and rate by stage, dividing `outcome="abandoned"` by reached volume;
3. failure volume and rate by stage, dividing `outcome="failed"` by reached plus failed attempts;
4. failure volume by `reason` and deploy annotation.

Open a release-blocking investigation when at least 20 attempts exist in the window and any of
these holds for 15 minutes: session-creation or room-entry failures exceed 5%, placement-completed
volume falls below 70% of room-joined volume, or first-attack volume falls below 70% of placement
volume. Match completion is monitored separately by mode because match duration and voluntary
surrender make a single global threshold misleading. Compare a candidate with the stable release
and the same acquisition/channel mix; do not compare raw counters across process restarts.

### Real-user performance

The browser reports one page-lifecycle sample for LCP, CLS, and INP plus one attack-command sample
for each authoritative result. Only fixed `route` (`landing`, `tutorial`, `lobby`, `join`, `room`,
`account`, `replay`, `other`) and `device_tier` (`desktop`, `mobile`, `low_mobile`) labels are
accepted. LCP, INP, and attack latency use milliseconds; CLS uses its unitless value multiplied by
1000. The server rejects unknown fields and out-of-range values, then immediately folds accepted
samples into cumulative histograms:

- `mk01_rum_lcp_milliseconds`;
- `mk01_rum_cls_milli`;
- `mk01_rum_inp_milliseconds`;
- `mk01_rum_battle_interaction_milliseconds`.

The dashboard must show sample volume and p75 by route/device for all three Web Vitals, plus p50,
p95, and p99 attack latency by device. Example LCP p75:

`histogram_quantile(0.75, sum by (le,route,device_tier) (rate(mk01_rum_lcp_milliseconds_bucket[1h])))`

Use the corresponding `_count` increase as the denominator guard. The good release targets are LCP
p75 ≤ 2500 ms, CLS p75 ≤ 100 milli, INP p75 ≤ 200 ms, and battle-interaction p95 ≤ 750 ms. Open a
release-blocking investigation when at least 100 Web Vital or 50 battle samples exist for the same
route/tier and LCP exceeds 4000 ms, CLS exceeds 250 milli, INP exceeds 500 ms, or battle p95 exceeds
1500 ms for 15 minutes. Before a canary reaches 100%, retain at least 100 landing samples for each
supported device tier over the rolling seven-day window; low-volume tiers require an explicit QA
traffic run rather than silently dropping the gate.

The browser calculates CLS using the maximum 1-second-gap/5-second session window and INP from the
98th-percentile interaction duration. Attack timing starts only when `attack:fire` is sent and ends
on the matching authoritative `attack:result`; the request ID never leaves browser memory through
the telemetry endpoint. Individual samples are never persisted by the application and must not be
joined to access logs or used for player scoring, moderation, or anti-cheat decisions.

Minimum paging alerts:

- readiness is non-200 for two minutes in two or more instances;
- API 5xx ratio exceeds 2% for five minutes;
- `rate(mk01_distributed_event_failures_total[5m]) > 0` in production;
- no room mutation succeeds for five minutes while accepted commands are non-zero;
- matchmaking p95 exceeds 60 seconds for ten minutes or the oldest queue entry exceeds 120 seconds;
- unexpected disconnect rate exceeds 2% for five minutes;
- PostgreSQL replication, disk, connection saturation, or backup age violates provider limits.

Ticket alerts cover a single-instance readiness failure, sustained rate-limit growth, elevated
version conflicts, funnel or RUM threshold violations, bundle-budget regression attempts, and
backup age above 12 hours.

The deployable alert source is `ops/observability/prometheus-rules.json`; Alertmanager routing in
the same directory sends `severity=page` to the on-call bridge and `severity=ticket` to the owned
work queue. The versioned Grafana dashboard provides the required availability, command,
matchmaking, disconnect, recovery, distributed-correctness, Web Vital, battle interaction, funnel,
retention and backup panels. `npm run observability:check` cross-checks every expression against the
application metric surface, while CI's official `promtool check rules` and `amtool check-config`
jobs reject invalid PromQL and alert routing.
Environment deployment must deliver a synthetic page and retain the receipt before production
promotion; repository validation cannot prove pager-provider connectivity.

The complete code-split artifact and the production gameplay journey share the versioned limits in
`config/performance-budgets.json`. `npm run budget` gates JavaScript, CSS, WOFF2 fonts, images, and
audio; `npm run test:performance` gates decoded route transfer, heap, CPU tasks, long tasks, frame
p95, and WebSocket bytes on desktop, 3× CPU mobile, and 6× CPU low-mobile tiers. Both are
release-blocking checks. The generated Korean subsets currently reduce the complete WOFF2 artifact
from 1,091,828 to 483,304 bytes; `npm run fonts:check` enforces source-glyph coverage and a 450 KB
Korean-slice cap. Baselines, measurement definitions, and budget review rules are in
`PERFORMANCE_BUDGETS.md`.

## Deployment and rollback

1. Build one immutable server/web artifact and record its Git SHA, protocol version, migration
   list, dependency audit, test report, and bundle-budget output.
2. Verify migrations are additive and compatible with both the current and candidate server.
3. Deploy to staging, run a complete two-browser match, then terminate one server during an active
   turn and verify recovery plus event delivery through the remaining instance.
4. Deploy one canary instance. Hold for at least 15 minutes and compare availability, p95/p99,
   disconnects, version conflicts, and distributed-event failures with the stable pool.
5. Increase 10% → 25% → 50% → 100%. Stop automatically when any SLO burn-rate or correctness gate
   fails.
6. Roll back application artifacts before rolling back data. Database migrations require a tested
   forward-fix unless an explicitly rehearsed down migration is safe for mixed versions.

Active matches are protocol-version 2 snapshots. A release that cannot read and preserve that
snapshot must not share a pool with the current release.

## Live-content publish and rollback

Seasons, events, mission feature flags, and bounded reward tuning are stored as append-only
PostgreSQL revisions. Operators must fetch current history, obtain a second-person review, run the
server-side dry run, and publish against the same `expectedRevision`. A conflict means another
operator published first and must be reviewed; it is never safe to retry blindly.

After publish, confirm `/api/content/live` reports the intended active revision on more than one
instance and inspect a real profile for the season, event, mission state, and reward amount. Compare
`increase(mk01_live_content_published_total[15m])` with the approved change record. A rollback is a
new revision and must increase `mk01_live_content_rollbacks_total`; history is never edited or
deleted. If reward integrity is uncertain, publish `missionsEnabled=false` first, retain issued
ledger entries, and then investigate. Cross-instance revision disagreement, an undecodable payload,
or an unexpected reward is a correctness incident.

The complete field bounds, CLI commands, scheduling semantics, review checklist, kill switches, and
rollback procedure are in `LIVE_CONTENT_OPERATIONS.md`.

CI's `rolling_instance_replacement_recovers_and_advances_an_active_match` test is the automated
precondition for this drill: it writes an active match through one instance, marks a player
disconnected, creates a replacement instance from the shared stores, reconnects inside the SLO,
checks hidden-state filtering and protocol continuity, and commits the next authoritative attack.
The staging termination drill remains the required infrastructure-level proof before promotion.

## Dependency-failure drills

Run quarterly in staging and after changes to storage or coordination:

PostgreSQL is authoritative and its optional Redis cache abandons the initial connection after two
seconds. Pub/Sub initialization has the same two-second bound: a non-distributed deployment then
starts in single-instance mode, while `DISTRIBUTED_COORDINATION_REQUIRED=true` fails startup and
readiness instead of hanging or silently degrading.

1. Start at least two server instances with `DISTRIBUTED_COORDINATION_REQUIRED=true`.
2. Play an active match with each browser connected to a different instance.
3. Stop Redis. Confirm readiness fails, stale room writes remain fenced, and clients recover after
   Redis returns. No hidden state may appear in captured frames.
4. Terminate one server during a turn. Confirm the match recovers within the recovery SLO and the
   deadline is committed once.
5. Introduce PostgreSQL unavailability. Commands must fail safely without broadcasting an
   uncommitted local state. Restore PostgreSQL and verify the last committed revision.
6. Fill a WebSocket send queue with a non-reading client and verify it is removed without memory
   growth or impact to healthy clients.

Attach logs, metric captures, browser traces, timestamps, observed RTO, and follow-up issues to the
drill record.

## Backup and restore drill

Production PostgreSQL backups must be encrypted, automated, and retained under the approved data
policy. The initial production objectives are **RPO ≤ 12 hours** and **RTO ≤ 15 minutes**. Provider
continuous recovery/PITR is the primary control; `scripts/backup-postgres.sh` produces a portable
GPG AES-256 encrypted custom-format backup and checksum as the independent recovery path.
`scripts/export-deletion-ledger.sh` produces a separately encrypted, checksum-protected current
deletion ledger. `scripts/restore-postgres-drill.sh` refuses non-isolated database names, rejects
backups older than the RPO or deletion ledgers older than one hour by default, rejects corruption,
applies forward migrations and every deletion tombstone, verifies
relational/snapshot/privacy invariants, and writes timestamped JSON evidence. CI backs up a fixture
before account deletion and proves that ledger replay removes the resurrected account/session, with
stricter five-minute RPO and two-minute RTO fixture gates. At least quarterly in production-like
staging:

1. restore the latest full backup and point-in-time logs into an isolated staging database;
2. run migrations with the release artifact;
3. supply the latest independently stored deletion ledger and require zero remaining personal
   records for every tombstone;
4. verify session counts, active-room revisions, game-result counts, and matchmaking invariants;
5. recover sampled games through the public API and complete one restored active game;
6. record recovery point and recovery time against the approved RPO/RTO;
7. destroy the isolated copy using the provider's audited deletion workflow.

Never restore production tokens into an internet-accessible environment. Staging access must be
restricted and token hashes should be invalidated or replaced when the drill does not require them.

## Incident roles

- Incident commander owns severity, decisions, handoffs, and closure.
- Operations lead owns mitigation, rollback, infrastructure, and evidence preservation.
- Game integrity lead checks hidden-state exposure, duplicate outcomes, ranking/reward impact, and
  whether matchmaking must be disabled.
- Communications lead owns player/status/support updates on the published cadence.

Correctness incidents involving hidden information, duplicated rewards, or an invalid winner are
severity 1 even when aggregate availability remains high.

`INCIDENT_RESPONSE.md` is the full operational state machine, escalation and evidence runbook.
`templates/STATUS_UPDATE.md` fixes player-safe public update fields and next-update deadlines;
`templates/POSTMORTEM.md` fixes blameless impact, timeline, causal analysis, recovery verification,
and action ownership. SEV-1 updates occur every 30 minutes and SEV-2 every two hours until resolved,
including when there is no material change.
