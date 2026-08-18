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
- Execute PostgreSQL/Redis integration tests and the active-match rolling-replacement test.
- Hold the canary while stable and candidate instances both read/write the expanded schema.
- Abort on migration error, lock/replication SLO breach, stale room commit, or snapshot decode error.
- Roll back application artifacts first. Database rollback is a forward fix unless an independently
  rehearsed down migration is proven safe with both versions.

`mk01-server --migrate-only` applies embedded SQLx migrations without starting HTTP or Redis.
`mk01-server --verify-restore` decodes every room snapshot, compares embedded and relational
persistence revisions, checks migration status and orphan invariants, and emits a JSON count report.
These commands are used by the automated encrypted restore drill.
