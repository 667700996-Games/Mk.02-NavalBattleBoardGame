# ADR-0002: PostgreSQL authority and Redis coordination

- Status: Accepted
- Date: 2026-08-18
- Decision owner: `data-reliability`
- Reviewer roles: `architecture`, `service-platform`, `game-integrity`
- Last reviewed: 2026-08-18

## Context

Multiple application instances must recover rooms, match players exactly once, enforce shared limits,
and deliver events without allowing a paused process or cache partition to overwrite durable truth.
In-memory ownership alone cannot survive replacement; Redis alone does not provide the relational
transactions and retained history required by results, privacy, ranking, and restore.

## Decision

PostgreSQL is the source of truth for sessions, rooms/snapshots, revisions, deadlines, queues, results,
ranked projections, safety evidence, live content, and privacy tombstones. Room writes use monotonic
revision CAS and, for distributed mutation, an expiring owner lease plus fencing token. Matchmaking
claims and pair completion are transactional.

Redis provides cross-instance pub/sub, shared rate limiting, cache acceleration, and coordination
health. Production may require Redis for readiness, but Redis never grants durable room authority and a
cache miss never becomes proof that data does not exist. Failure degrades delivery/readiness while
PostgreSQL fencing continues to reject stale writers.

## Rejected alternatives

- Process memory as authority: loses matches and timers on replacement.
- Redis as the only durable store: weakens relational settlement, privacy deletion, and restore proof.
- Distributed locks without persistence revision: a delayed holder can still commit stale state.
- Database writes followed by untracked best-effort behavior: hides fan-out failure from operations.

## Consequences

Production needs both managed PostgreSQL and Redis, service-backed tests, encrypted restore drills, and
observability. Application code must tolerate cache loss but fail readiness when required coordination
is unhealthy. Schema changes use expand/migrate/contract and remain readable by stable/candidate
artifacts.

## Verification

The serialized PostgreSQL/Redis suite proves stale-owner fencing, concurrent revision CAS, atomic
matchmaking, fan-out, optional-cache failure, active-match replacement, migration compatibility,
privacy deletion, and restore invariants. Operational alerts cover coordination failures and recovery.

## Review triggers

Review before a new datastore, multi-region write authority, event-stream replacement, changed
consistency model, cache write-behind, or any relaxation of PostgreSQL fencing/readiness.
