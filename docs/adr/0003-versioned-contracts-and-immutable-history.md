# ADR-0003: Versioned contracts and immutable interpretation history

- Status: Accepted
- Date: 2026-08-18
- Decision owner: `service-platform`
- Reviewer roles: `architecture`, `web-experience`, `data-reliability`, `game-integrity`
- Last reviewed: 2026-08-18

## Context

Rolling releases combine stable and candidate servers, cached web clients, active matches, additive
database schemas, live-content revisions, and historical replays. One global version cannot describe
all compatibility axes, and rewriting old data makes results or replays impossible to interpret.

## Decision

Protocol, database schema, balance rules, and live content have independent versions and policies.
Protocol manifests/fixtures and migration files are checksum-frozen. Balance manifests and live
content are append-only revisions. Rooms, snapshots, results, and replays retain the exact protocol
and balance pins used when play occurred.

Breaking wire changes create a new protocol with a dual-version server window. Schema changes use
expand/migrate/contract. Balance/live changes publish a new immutable revision and rollback by
selecting or publishing a successor, never by editing history. Old adapters are removed only after the
minimum compatibility period, zero old traffic, and active-match drain.

## Rejected alternatives

- One application version for every axis: couples safe content/schema changes to client upgrades.
- Editing old manifests or migrations: destroys replay, restore, and audit evidence.
- Client refresh as the only rollout strategy: abandons active matches and cached clients.
- Destructive migration rollback: can corrupt data still used by stable instances.

## Consequences

Artifacts and adapters consume storage and maintenance effort. Release evidence must include contract,
migration, balance, mixed-version, browser, and restore gates. Historical code paths remain until
explicit retirement conditions pass.

## Verification

Checksum tools reject drift; frozen client fixtures deserialize through every supported server window;
HTTP/WebSocket integration covers explicit and legacy negotiation; database tests cover future additive
schema; replay/result tests retain exact pins; restore verification checks migrations and manifests.

## Review triggers

Review before a protocol bump, version-window policy change, migration policy change, mutable content
proposal, new replay visibility, or removal of any frozen adapter or artifact.
