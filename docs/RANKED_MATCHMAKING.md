# Ranked Matchmaking Policy

Mk.01 ranked matchmaking is a server-authoritative, durable 1v1 queue. This document defines the
shipping constraints for the A2 matchmaking gate. Ranked progression, placements, seasonal tier
movement, decay, rewards, and leaderboards remain separate C2 work and are not implied by this
policy.

## Authority boundary

| Input | Authority | Enforcement |
| --- | --- | --- |
| Queue pool | Player selects casual or ranked | Strict enum and queue-row check constraint |
| Region | Player selects one supported ranked region | `AUTO` is rejected for ranked play |
| Reported RTT | Browser probes the active service edge before queueing | Server accepts only 1–300 ms and applies the current search ceiling |
| Rating | `ranked_ratings` in the authoritative store | Client rating fields are rejected; queue inserts are compared with the stored value |
| Party identity | Authenticated account ID | Client party fields are rejected; two sessions for one account cannot match each other |
| Party size | Server-fixed value of one | Mk.01 is 1v1; database and domain constraints reject any other size |
| Wait time | Durable queue timestamp | A repeated enqueue is idempotent and cannot reset or replace its search profile |

An empty `POST /api/matchmaking` remains the backward-compatible casual request. Ranked clients
use the additive `POST /api/matchmaking/ranked` route so a request routed to a stable-version
instance fails safely instead of being interpreted as casual. They send only:

```json
{
  "pool": "RANKED",
  "region": "KOREA",
  "latencyMs": 55
}
```

Unknown fields are rejected, including `rating`, `partyId`, and `partySize`. Guests receive
`RANKED_ACCOUNT_REQUIRED`. A queued player must cancel before changing pool, region, or latency.

## Mutual widening

Every candidate must satisfy both players' current windows. One player's longer wait never widens
the other player's consented search window.

| Elapsed time for both players | Phase | Rating delta | RTT ceiling | Region scope |
| --- | --- | ---: | ---: | --- |
| 0–29 seconds | `EXACT` | ±100 | 120 ms | Exact region |
| 30–89 seconds | `REGIONAL` | ±250 | 200 ms | Exact region or the same regional group |
| 90+ seconds | `GLOBAL` | ±500 | 300 ms | Any supported region |

The Asia-Pacific group is Korea, Japan, and Southeast Asia. North America West and East form the
North America group; Europe is its own group. Match quality persisted with the room exposes the
effective shared phase, rating delta, maximum reported RTT, pool, and party size without exposing
account or party identifiers.

The lobby polls its existing idempotent ticket every three seconds. Polling does not reset
`queued_at`; it re-evaluates all durable candidates so a pair can become eligible when its mutual
window advances even if no new player joins the queue.

## Distributed correctness and compatibility

- PostgreSQL owns the queue, ratings, timestamps, claims, and constraints. Redis is not required
  for a correct match decision.
- Candidate selection locks only an eligible queue row with `FOR UPDATE ... SKIP LOCKED`; the pair
  claim updates both rows atomically and room creation consumes exactly those two rows.
- Bidirectional player blocks are checked during candidate selection.
- A stale claim is released after 30 seconds and an abandoned unclaimed ticket after ten minutes.
- Migration `202608180004_ranked_matchmaking.sql` is additive. Stable-version casual inserts that
  specify only `session_id` and `queued_at` remain valid and decode as solo casual tickets during a
  rolling deployment.
- Ranked HTTP traffic uses a new route during the mixed-version window; the shared casual route
  rejects request bodies on candidate instances.
- Account exports include the optional ranked rating record, while account deletion removes it by
  foreign-key cascade and removes every account session from the queue.

## Operations and acceptance evidence

Prometheus exports total and ranked queued/completed counters, total and ranked queue depth, oldest
ticket age, and the existing queue-to-room latency histogram. Operators should compare ranked queue
depth with phase distribution and the global matchmaking p95 before changing the widening policy.

Acceptance coverage includes domain boundary tests, memory-store authority tests, API rejection
and response-contract tests, a real PostgreSQL/Redis two-instance test, legacy-write migration
compatibility, and Chromium/Firefox/WebKit lobby tests. The primary commands are:

```sh
cargo test -p mk01-server ranked
TEST_DATABASE_URL=... TEST_REDIS_URL=... \
  cargo test -p mk01-server --test distributed_postgres -- --test-threads=1
npm --workspace @mk01/web run test:e2e -- e2e/ranked-matchmaking.spec.ts
```
