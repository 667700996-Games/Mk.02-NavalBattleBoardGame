# Ranked Leaderboard Policy

Mk.01 ranked leaderboards are authenticated, server-authoritative views over settled seasonal
results. Clients can select a published season, page size, or opaque cursor; they cannot submit a
rating, rank key, account identity, eligibility flag, snapshot time, or moderation state.

## Eligibility and deterministic order

An account enters a snapshot only when all of these invariants hold:

- at least five placement matches have settled;
- wins plus losses equals the standing's match count;
- the match count equals the number of authoritative `ranked_match_participants` rows joined to
  settlement rows for that same season; and
- the account still exists.

Rows order by rating descending, wins descending, peak rating descending, matches played ascending,
and internal account UUID ascending. The UUID is only a deterministic final tie-breaker and is
never serialized. Sequential rank, handle, rating, tier, match totals, and peak rating are the only
public entry fields. Provisional or projection-only rows therefore cannot be promoted by legacy
data, client input, duplicate settlement, or a partially written result.

## Snapshot pagination and seasonal archive

The first active-season request creates a PostgreSQL snapshot with a random UUID and five-minute
expiry. Each next-page cursor is a separate random UUID stored with its snapshot and last rank; the
client never receives the ordering key or an account ID. Invalid, expired, cross-season, or invented
cursors fail closed. All pages from a cursor chain read the same ratings and ranks even while new
matches settle.

After a season ends, a 24-hour result-finalization window continues to use expiring snapshots so a
match started near the boundary can settle. The first request after that window atomically creates
one immutable archive snapshot per season. Concurrent instances converge through a partial unique
index, and later standing changes cannot rewrite the archive. Archived entry rows remain subject to
account deletion and live privacy/moderation filtering.

## Privacy and competitive integrity

- The endpoint requires an authenticated account and inherits per-session/IP rate limits.
- `leaderboard_visible` is an account-owned preference. Players can opt out or back in from the
  stats view; the choice is applied at read time, exported with account data, and deleted with the
  account.
- Responses contain a handle but no account UUID, session UUID, cursor internals, last-online time,
  exact match timestamps, report state, or moderation evidence.
- Active bans and unexpired suspensions targeting the account or any of its sessions are removed at
  read time. A recorded reversal restores eligibility without rebuilding the snapshot.
- Account deletion cascades through current standings and snapshot entries. Restore verification
  checks snapshot/cursor references and rejects any leaderboard entry resurrected for a deletion
  tombstone.

## Operations and acceptance

Prometheus exports successful requests, empty pages, entries served, and visibility changes without
player or season labels. The `Ranked leaderboard integrity` dashboard compares those counters. A
ticket fires when more than 75% of at least 20 pages are empty while at least 20 ranked pairs also
completed in 30 minutes; operators inspect placement eligibility, settlement counts, moderation
volume, privacy opt-outs, and snapshot creation before intervention.

Acceptance requires domain/memory tests, authenticated API/privacy export tests, real PostgreSQL
snapshot/cursor/penalty tests, deletion/restore checks, and Chromium/Firefox/WebKit stats-view
coverage. Migration `202608180007_ranked_leaderboards.sql` is additive and stable servers ignore all
new columns and tables during a rolling deployment.
