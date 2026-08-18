# Mk.01 Service Operations Baseline

This runbook defines the first production service-level gates for Mk.01. It is intentionally
executable: a release is not production-ready until the staging drills below have produced dated
evidence owned by the release team.

## Initial SLOs

| Signal                  | Objective                             | Measurement                                                                  |
| ----------------------- | ------------------------------------- | ---------------------------------------------------------------------------- |
| API availability        | 99.95% per rolling 30 days            | non-5xx responses excluding `/api/health`                                    |
| Command latency         | p95 < 150 ms, p99 < 400 ms            | authenticated HTTP and accepted WebSocket command spans                      |
| Matchmaking latency     | p95 < 30 s in a healthy regional pool | enqueue to durable room assignment                                           |
| Unexpected disconnects  | < 0.5% of active-player hours         | abnormal socket closes / connected player hours                              |
| Active-match recovery   | p95 < 10 s after instance loss        | last accepted room revision to first recovered snapshot                      |
| Persistence correctness | zero stale commits                    | `mk01_room_version_conflicts_total` may rise; stale commits must remain zero |

The first two 30-day windows are calibration windows. Any objective change requires a dated ADR;
silently relaxing an alert is not allowed.

## Metrics and minimum alerts

The server exposes Prometheus text at `/api/metrics`. The endpoint currently includes request and
rate-limit totals, local WebSocket connections/events, distributed publish success/failure, room
mutation/version-conflict and authority acquisition/conflict totals, matchmaking
queue/completion/cancellation totals, current queue depth, oldest queue-entry age, and retention
deletion totals for sessions, completed rooms, abandoned queue entries, closed moderation cases,
integrity signals, and the privacy-preserving new-player funnel below.

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

Minimum paging alerts:

- readiness is non-200 for two minutes in two or more instances;
- API 5xx ratio exceeds 2% for five minutes;
- `rate(mk01_distributed_event_failures_total[5m]) > 0` in production;
- no room mutation succeeds for five minutes while accepted commands are non-zero;
- matchmaking p95 exceeds 60 seconds for ten minutes or the oldest queue entry exceeds 120 seconds;
- unexpected disconnect rate exceeds 2% for five minutes;
- PostgreSQL replication, disk, connection saturation, or backup age violates provider limits.

Ticket alerts cover a single-instance readiness failure, sustained rate-limit growth, elevated
version conflicts, funnel threshold violations, bundle-budget regression attempts, and backup age
above 12 hours.

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

CI's `rolling_instance_replacement_recovers_and_advances_an_active_match` test is the automated
precondition for this drill: it writes an active match through one instance, marks a player
disconnected, creates a replacement instance from the shared stores, reconnects inside the SLO,
checks hidden-state filtering and protocol continuity, and commits the next authoritative attack.
The staging termination drill remains the required infrastructure-level proof before promotion.

## Dependency-failure drills

Run quarterly in staging and after changes to storage or coordination:

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
