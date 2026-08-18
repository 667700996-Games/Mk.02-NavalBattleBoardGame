# Mk.01 Service Incident Response

This runbook governs availability, latency, dependency, matchmaking, game-integrity, data, and
player-experience incidents. Security vulnerabilities follow `VULNERABILITY_RESPONSE.md`; if an
incident may be both operational and security-sensitive, use the stricter clock and keep technical
details in the restricted security record.

The machine-readable clocks, required roles, dashboard panels, alerts, and templates live in
`.github/incident-response-policy.json`. `npm run observability:check` prevents an alert, dashboard,
or response artifact from silently losing a required signal or owner.

## Severity and declaration

| Severity | Examples | Acknowledge | Public update cadence | Review |
| --- | --- | ---: | ---: | ---: |
| SEV-1 | hidden fleet or winner corruption, broad outage, data loss, unrecoverable active matches | 15 min | 30 min | postmortem within 5 business days |
| SEV-2 | material regional degradation, SLO paging alert, delayed recovery, matchmaking unavailable | 30 min | 2 hours | postmortem within 5 business days |
| SEV-3 | narrow degradation with a safe workaround, ticket alert without player-wide impact | 4 hours (240 min) | 8 hours while active | review within 10 business days |

Any hidden-information disclosure, invalid winner, duplicate durable reward, or stale authoritative
commit is SEV-1 even if aggregate availability remains high. The incident commander may raise
severity immediately. Lowering severity requires timestamped evidence that the higher-impact
hypothesis was disproved.

## Required roles

| Role | Accountable work |
| --- | --- |
| Incident commander | declares severity, owns decisions and handoffs, approves recovery and closure |
| Operations lead | mitigation, rollback, dependencies, capacity, evidence capture and recovery validation |
| Game integrity lead | hidden state, winner, replay, reward, ranking, matchmaking and active-match impact |
| Communications lead | public status, player/support briefings, cadence and confirmed-scope language |
| Scribe | immutable timeline, hypotheses, decision log, commands, evidence links and action items |

The production on-call is incident commander until a handoff is recorded. One responder may cover
more than one role initially, but every role must have a named owner before SEV-1 containment or the
first SEV-2 public update. The incident commander assigns security, privacy, database, or customer
support specialists when those boundaries are involved.

## Response state machine

1. **Detect and declare.** Acknowledge the page, open a restricted incident record, assign the five
   roles, set severity, record the earliest known impact time, deployment SHA, environment, affected
   regions/modes, and links to the Mk.01 Grafana dashboard and firing alerts.
2. **Bound the impact.** Compare stable and candidate deployments. Check API error budget, command
   p95/p99, queue age, disconnects, recovery, authority/version conflicts, distributed delivery,
   RUM, funnel, PostgreSQL, Redis, and encrypted-backup age. State what is known, suspected, and
   explicitly disproved.
3. **Contain safely.** Prefer a reversible application rollback, canary removal, traffic shift,
   matchmaking pause, reward pause, or feature flag. Never edit player or room rows directly.
   Preserve the last durable room revision and deletion ledger. A data migration is forward-fixed
   unless a rehearsed down migration is proven safe for both running versions.
4. **Stabilize and recover.** Follow the dependency and backup drills in `OPERATIONS.md`. Complete a
   synthetic match when game state is affected. Verify hidden-state filtering, one winner, one
   reward, monotonic room revisions, client/server protocol compatibility, and reconnect deadlines
   before restoring full traffic.
5. **Communicate.** Start from `templates/STATUS_UPDATE.md`. Publish only confirmed impact and player
   actions; never publish credentials, exploit detail, fleet coordinates, chat, personal data, raw
   traces, or internal hostnames. Post at the severity cadence even when there is no material change.
6. **Resolve and monitor.** Resolution requires the affected SLOs to remain inside objective for at
   least 30 minutes, correctness counters to remain clean, the canary/full rollout decision to be
   recorded, and player recovery instructions to be verified. State residual risk and the next
   update or explicitly close the public incident.
7. **Learn.** Copy `templates/POSTMORTEM.md`. SEV-1/2 reviews are blameless, include the full timeline,
   root and contributing causes, detection and response gaps, player/data/game-integrity impact,
   and risk-ranked actions with one owner and due date each. Link evidence; do not paste secrets.

## Alert routing and escalation

Prometheus rules under `ops/observability/prometheus-rules.json` use `severity=page` for immediate
on-call delivery and `severity=ticket` for owned follow-up. Alertmanager must route `service=mk01`
pages to the production on-call and simultaneously notify the incident channel. If a page is not
acknowledged within the severity clock, escalate to the engineering lead and game-integrity owner.
Every alert annotation links to the relevant section of `OPERATIONS.md`.

The versioned Grafana dashboard at
`ops/observability/grafana/dashboards/mk01-service.json` is the common view for responders, support,
and release owners. Deploy annotations and incident annotations must use timestamps and artifact or
incident IDs, not player identity.

## Evidence and access rules

The restricted incident record must retain role assignments, severity changes, alert snapshots,
queries, sanitized logs/traces, deployment and rollback SHAs, commands and outcomes, public updates,
player-support guidance, recovery verification, and follow-up actions. Use synthetic accounts for
reproduction. Never copy session/recovery tokens, personal chat, raw backups, or hidden opponent
state into tickets, dashboards, public status, or postmortems.

Operational and moderation actions must use authenticated product tooling and its audit trail.
Direct production database changes are prohibited. Emergency database access requires a separate
break-glass record, two-person approval, read-only credentials by default, and a post-incident access
review.
