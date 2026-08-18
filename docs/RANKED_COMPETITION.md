# Ranked Competition Policy

Mk.01 ranked competition is a server-authoritative seasonal system. The active immutable live-
content season controls entry, the room snapshot pins that season at match creation, and the
authoritative result transaction settles rating and rewards exactly once.

## Player lifecycle

- Ranked play requires an authenticated account and an active season window. A scheduled or ended
  season returns `RANKED_SEASON_UNAVAILABLE`; casual and private play remain available.
- Each season starts with five placement matches. Rating changes during placement, but the public
  tier remains `PROVISIONAL` until all five results settle.
- A new season soft-resets rating halfway toward 1500, clamped to 1000–2000, then starts a fresh
  placement series. Every prior standing remains archived by account and season.
- The profile projection exposes season, rating, matches, wins, losses, peak rating, tier,
  placements remaining, last match, next decay, applied decay, and ranked reward XP. The stats
  view presents the active tier and rating without exposing matchmaking identity.

## Rating and tiers

Results use expected-score rating with a 400-point scale. Placement K-factor is 64; established
K-factor is 32. A win gains at least one point and a loss loses at least one point, with every
result clamped to the supported 0–4000 range.

| State/rating         | Tier          |
| -------------------- | ------------- |
| Fewer than 5 matches | `PROVISIONAL` |
| 0–1199               | `BRONZE`      |
| 1200–1499            | `SILVER`      |
| 1500–1799            | `GOLD`        |
| 1800–2099            | `PLATINUM`    |
| 2100–2399            | `DIAMOND`     |
| 2400–4000            | `ADMIRAL`     |

The settlement reads both locked pre-match standings before computing either update. Concurrent
finishes therefore serialize without lost updates, and the current `ranked_ratings` projection is
updated in the same transaction for subsequent matchmaking.

## Inactivity and rewards

Established players at 2100 RP or above receive a 14-day grace period. Each additional seven-day
step deducts 25 RP down to a floor of 1800. The standing stores applied decay steps, making repeated
profile or queue reads idempotent. Playing a ranked match resets the inactivity schedule.

Rewards are non-pay-to-win progression XP in a dedicated immutable ledger:

| Source                                    |                 XP |
| ----------------------------------------- | -----------------: |
| Ranked win                                |                100 |
| Ranked loss                               |                 40 |
| Five-placement completion                 |                500 |
| Prior season Bronze / Silver / Gold       |   500 / 750 / 1000 |
| Prior season Platinum / Diamond / Admiral | 1500 / 2000 / 3000 |

Match rewards are unique by account, room, source, and season. Placement and season rewards are
unique by account and season. A prior season reward is issued only after a genuinely later active
season begins: the prior standing's last match must predate the new season start. This prevents a
live-content rollback from prematurely closing a newer standing.

## Transaction and compatibility boundary

`202608180005_ranked_competition.sql` is additive. It introduces seasonal standings, a room-level
settlement marker, per-account rating deltas, ranked rewards, and an optional queue season key.
Stable-version queue inserts remain valid during a rolling deployment. A legacy ranked row with no
season key can be restored or drained but is excluded from new candidate selection.

When a ranked room first persists a result, PostgreSQL performs these operations in one transaction:

1. persist the authoritative room and result;
2. insert the unique room settlement marker;
3. lock both seasonal standings in account order;
4. calculate and write both rating outcomes;
5. update matchmaking rating projections;
6. append participant deltas and idempotent reward rows.

Any failure rolls back the room result and every ranked mutation. Re-saving a finished room sees
the marker and cannot rate or reward it twice. Redis is only cache/fan-out and is not part of the
correctness boundary.

## Privacy, recovery, and acceptance

Account export contains current rating, all seasonal standings, per-match deltas, ranked rewards,
and leaderboard visibility. Verified account deletion removes every account-bound ranked row by
explicit deletion or foreign-key cascade; anonymous settlement markers retain no account identity.
Restore verification checks all 21 migrations, queue profiles, ranked and leaderboard table counts,
references, and deletion tombstones.

Acceptance evidence includes domain and memory-store tests, API profile/snapshot contracts, a fresh
PostgreSQL 16 migration, the PostgreSQL/Redis distributed suite, restore verification,
Chromium/Firefox/WebKit player flow, and the bundle/performance gates. Rematch policy is defined in
`RANKED_MATCHMAKING.md`; public ranking policy is defined in `RANKED_LEADERBOARDS.md`.
