# Ranked Matchmaking Policy

Mk.01 ranked matchmaking is a server-authoritative, durable 1v1 queue. This document defines the
shipping constraints for the A2 matchmaking gate. Seasonal rating, placements, tiers, inactivity,
and reward settlement are defined in `RANKED_COMPETITION.md`; public seasonal ranking is defined in
`RANKED_LEADERBOARDS.md`.

## Authority boundary

| Input | Authority | Enforcement |
| --- | --- | --- |
| Queue pool | Player selects casual or ranked | Strict enum and queue-row check constraint |
| Region | Player selects one supported ranked region | `AUTO` is rejected for ranked play |
| Reported RTT | Browser probes the active service edge before queueing | Server accepts only 1–300 ms and applies the current search ceiling |
| Rating | Active `ranked_season_standings` projection | Client rating fields are rejected; queue inserts are compared with the server projection |
| Party identity | Authenticated account ID | Client party fields are rejected; two sessions for one account cannot match each other |
| Party size | Server-fixed value of one | Mk.01 is 1v1; database and domain constraints reject any other size |
| Wait time | Durable queue timestamp | A repeated enqueue is idempotent and cannot reset or replace its search profile |
| Recent opponents | Authoritative retained results | Aggregate pair history is derived server-side; no opponent identifier is accepted from or exposed to a client |

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
account or party identifiers. It also records recent pairing count, whether rematch relaxation was
required, the mutual shared wait, and wait skew as identity-free audit evidence.

The lobby polls its existing idempotent ticket every three seconds. Polling does not reset
`queued_at`; it re-evaluates all durable candidates so a pair can become eligible when its mutual
window advances even if no new player joins the queue.

## Recent-opponent control and starvation escape

Ranked selection counts authoritative results between two account/session identities during the
previous 30 minutes. A recent opponent is excluded until **both** tickets have waited 90 seconds
and reached `GLOBAL`. From 90 through 179 seconds, a novel opponent always sorts ahead of an
eligible rematch even when the rematch ticket is older. At 180 seconds of mutual wait the rematch
penalty becomes zero and FIFO order is restored, preventing a small regional pool from starving.

Within the same rematch-priority class, the oldest candidate wins; rating delta and RTT are stable
tie-breakers. Thus queue age remains the primary fairness rule without letting an immediate repeat
consume a healthy alternative. The query uses retained result/participant rows and additive
identity/result indexes. Account deletion anonymizes those participant identities through the
existing verified privacy workflow, so no separate opponent-history profile exists.

## Distributed correctness and compatibility

- PostgreSQL owns the queue, ratings, timestamps, claims, and constraints. Redis is not required
  for a correct match decision.
- Ranked tickets carry a server-derived season key and only match the same active season. The room
  pins the human season ID and content revision for result settlement.
- Candidate selection locks only an eligible queue row with `FOR UPDATE ... SKIP LOCKED`; the pair
  claim updates both rows atomically and room creation consumes exactly those two rows.
- Bidirectional player blocks are checked during candidate selection.
- A stale claim is released after 30 seconds and an abandoned unclaimed ticket after ten minutes.
- Migration `202608180004_ranked_matchmaking.sql` is additive. Stable-version casual inserts that
  specify only `session_id` and `queued_at` remain valid and decode as solo casual tickets during a
  rolling deployment.
- Migration `202608180005_ranked_competition.sql` adds an optional season key. Legacy ranked rows
  without it remain readable for drain/restore but cannot pair with a new seasonal ticket.
- Migration `202608180006_matchmaking_fairness.sql` adds only recent-result participant indexes;
  stable servers neither read nor write a new representation.
- Ranked HTTP traffic uses a new route during the mixed-version window; the shared casual route
  rejects request bodies on candidate instances.
- Account exports include rating, seasonal standings, result deltas, and rewards. Account deletion
  removes all account-bound ranked records and every account session from the queue.

## Operations and acceptance evidence

Prometheus exports total and ranked queued/completed counters, relaxed-rematch completions, total
and ranked queue depth, oldest ticket age, and the existing queue-to-room latency histogram. The
dashboard graphs the 30-minute relaxed-rematch share and tickets when it exceeds 25% with at least
20 ranked completions. Operators should compare this share, ranked depth, and global matchmaking
p95 before changing the widening policy.

Acceptance coverage includes domain boundary tests, memory-store history/priority tests, API
rejection and response-contract tests, real PostgreSQL/Redis history selection across instances,
legacy-write migration compatibility, and Chromium/Firefox/WebKit lobby tests. The primary
commands are:

```sh
cargo test -p mk01-server ranked
TEST_DATABASE_URL=... TEST_REDIS_URL=... \
  cargo test -p mk01-server --test distributed_postgres -- --test-threads=1
npm --workspace @mk01/web run test:e2e -- e2e/ranked-matchmaking.spec.ts
```
