# Database Migration Compatibility Policy

Mk.01 deploys application versions side by side during canary and rolling releases. A database
migration therefore has to remain readable and writable by both the stable and candidate server
for the entire mixed-version window.

## Required expand/migrate/contract sequence

1. **Expand:** add nullable columns, columns with backward-compatible defaults, new tables, and new
   indexes. Deploy code that can read both the old and expanded representation.
2. **Migrate:** backfill in bounded, resumable batches while observing locks, replication lag, and
   command latency. Reads must continue to accept the old representation during this phase.
3. **Contract:** only after every old server is gone and the compatibility window has expired may a
   later release enforce a constraint or remove the old representation. Destructive SQL is never
   combined with the code release that stops using it.

Applied migration files are immutable. `checksums.sha256` records their approved content and
`npm run migrations:check` rejects edits, removal, non-monotonic names, destructive DDL, type or
rename operations, and new `NOT NULL` columns without a compatible default. A correction is always
a new forward migration.

## Release procedure

- Run migrations before the canary starts and record their versions in the release evidence.
- Balance rulesets are append-only catalog rows. Never update/delete an old manifest or reuse its
  version/checksum; follow `BALANCE_VERSIONING.md` and drain active rooms before changing current.
- Execute `npm run test:distributed`; CI requires real PostgreSQL/Redis URLs, serializes the suite,
  and retains its post-test `--verify-restore` report as described in `DISTRIBUTED_INTEGRATION.md`.
- Hold the canary while stable and candidate instances both read/write the expanded schema.
- Abort on migration error, lock/replication SLO breach, stale room commit, or snapshot decode error.
- Roll back application artifacts first. Database rollback is a forward fix unless an independently
  rehearsed down migration is proven safe with both versions.

`mk01-server --migrate-only` applies embedded SQLx migrations without starting HTTP or Redis.
`mk01-server --verify-restore` decodes every room snapshot, compares embedded and relational
persistence revisions and balance pins, validates the append-only balance catalog and result
manifests, checks migration status and orphan invariants, and emits a JSON count report.
It also decodes every durable matchmaking profile, including rolling-compatible legacy casual and
pre-season-key ranked rows, and reports queue, rating, seasonal-standing, settlement, and ranked-
reward counts.
The matchmaking fairness expansion adds only composite indexes over retained result timestamps and
participant identities. Candidate servers use those indexes for a bounded 30-minute recent-
opponent query; stable servers continue using the same result and participant rows unchanged.
The leaderboard expansion adds a default-visible account preference plus isolated snapshot, entry,
and opaque-cursor tables. Stable servers ignore them. Candidate servers retain active snapshots for
five minutes and one finalized archive per past season; account foreign keys cascade deleted player
entries without storing handles or other personal data in the archive.
It also decodes every live-content payload and compares its revision, schema, activation, operator,
change note, rollback source, and creation timestamp with the immutable relational audit columns.
It also fails if a deletion tombstone has any surviving account, session, reward, participant, or
result identity. `--export-deletion-ledger` and `--apply-deletion-ledger <file>` provide the
encrypted restore workflow with idempotent backup-deletion replay. These commands are used by the
automated encrypted restore drill.
