# PostgreSQL and Redis integration gate

Mk.01 treats the service-backed suite as a release gate, not as an optional extension of the memory
tests. The dedicated `postgres-redis-integration` CI job starts PostgreSQL 16 and Redis 7 health-
checked containers, applies the embedded migration chain, runs every distributed test serially, and
uploads a database-verification report. Browser and backup/restore jobs cannot start until this job
passes.

## Fail-closed execution

`npm run test:distributed` runs only `distributed_postgres.rs` with one test thread. The tests share
one real database and deliberately race application instances inside individual cases, so runner-
level parallelism would add unrelated truncate/migration races and weaken diagnosis.

The CI job sets all three variables below:

```sh
TEST_DATABASE_URL=postgres://...
TEST_REDIS_URL=redis://...
REQUIRE_DISTRIBUTED_INTEGRATION=true
npm run test:distributed
```

Without the two service URLs, developers may run `npm test` and the service-backed cases report an
explicit local skip. With `REQUIRE_DISTRIBUTED_INTEGRATION=true`, a missing URL panics before a test
can skip. This prevents a renamed secret, removed service, or workflow refactor from producing a
false green CI result.

## Covered failure and concurrency boundaries

The twelve cases prove these controls against actual PostgreSQL and Redis protocols:

- all immutable SQLx migrations apply to an empty PostgreSQL 16 database;
- a stable binary fails on known checksum drift but restarts after an unknown future additive
  migration; stable and candidate session/room/result projections remain mutually readable, the
  legacy-result dual write populates candidate identity indexes, and the same rule covers normal
  startup, migrate-only, deletion replay, and restore verification;
- persistence-revision CAS and room-owner fencing admit one concurrent writer;
- distributed matchmaking claims and creates each pair exactly once;
- Redis Pub/Sub delivers events between separate store instances;
- PostgreSQL remains authoritative when the optional Redis cache is unreachable;
- an active match survives instance replacement and advances from its persisted deadline;
- account export/deletion, immutable balance pins, live-content CAS, ranked settlement, rematch
  fairness, and leaderboard privacy retain their relational invariants.

The privacy case also expires the account's last device session before deletion. It proves the
durable result participant index still finds and anonymizes room/result copies, exports every
derived identity class, strips result UUID arrays, deletes direct-target moderation actions, evicts
the Redis room cache, and preserves unrelated moderation evidence.

After the suite, `mk01-server --verify-restore` decodes every retained room and matchmaking snapshot,
compares relational revisions and balance pins, checks migrations and orphan references, and writes
JSON evidence. CI derives the expected migration count from `migrations/checksums.sha256`, requires
rooms and results from the real suite, requires at least one balance catalog entry, and retains the
report for 90 days as `postgres-redis-integration-<commit>`.

The August 18, 2026 clean-service parity run passed 12/12 cases in 10.77 seconds. Its verifier
decoded 20 migrations, 18 sessions, 22 rooms, 16 results, 15 ranked settlements, 30 ranked rewards,
one balance catalog entry, four privacy requests, three deletion tombstones, and three live-content
revisions with no orphan, migration, snapshot, or resurrection failure.

## Release use and triage

A failed service health check, missing required URL, test failure, snapshot decode error, migration
count mismatch, empty result evidence, or artifact upload failure blocks dependent browser and
backup jobs. Do not retry by removing `REQUIRE_DISTRIBUTED_INTEGRATION` or enabling parallel test
threads. Reproduce with fresh local PostgreSQL/Redis databases, preserve the first failing output,
and distinguish infrastructure startup from an application invariant before retrying CI.

Migration changes must run this gate on an empty database and the separate encrypted restore drill
on a backed-up database. Redis-only failures do not permit a distributed production rollout: the
single-instance fallback is allowed only when distributed coordination is not required.
