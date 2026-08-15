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
mutation/version-conflict totals, and matchmaking queue/completion/cancellation totals.

Minimum paging alerts:

- readiness is non-200 for two minutes in two or more instances;
- API 5xx ratio exceeds 2% for five minutes;
- `rate(mk01_distributed_event_failures_total[5m]) > 0` in production;
- no room mutation succeeds for five minutes while accepted commands are non-zero;
- matchmaking p95 exceeds 60 seconds for ten minutes or the oldest queue entry exceeds 120 seconds;
- unexpected disconnect rate exceeds 2% for five minutes;
- PostgreSQL replication, disk, connection saturation, or backup age violates provider limits.

Ticket alerts cover a single-instance readiness failure, sustained rate-limit growth, elevated
version conflicts, bundle-budget regression attempts, and backup age above 12 hours.

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
policy. At least quarterly:

1. restore the latest full backup and point-in-time logs into an isolated staging database;
2. run migrations with the release artifact;
3. verify session counts, active-room revisions, game-result counts, and matchmaking invariants;
4. recover sampled games through the public API and complete one restored active game;
5. record recovery point and recovery time against the approved RPO/RTO;
6. destroy the isolated copy using the provider's audited deletion workflow.

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
