# Balance ruleset versioning

Mk.01 treats game balance as part of the authoritative match record, not as mutable presentation
configuration. Every room receives one `BalancePin` when it is created. The pin contains a ruleset
version, a SHA-256 checksum, and the complete interpretation manifest. It is copied into the active
game, snapshots, result history, and finished replay.

## What the manifest fixes

Manifest schema 1 records all values needed to interpret the current game:

- board dimensions and every ship kind/length;
- classic shot allowance and the surviving-ship salvo policy;
- rapid and maximum turn durations;
- the consecutive-timeout forfeit threshold;
- turn advance, duplicate-target, victory, and post-match fleet-reveal policies.

The Rust engine validates placement, targeting, turn time, salvo allowance, and timeout forfeits
against the room's pinned manifest. The web placement and board components consume the same server
manifest. Replay displays its exact fleet, timing, timeout, board, version, and full checksum rather
than consulting the latest defaults.

Protocol, room-state, live-content, and balance versions have separate meanings. `protocolVersion`
describes the transport shape, room `version` provides optimistic concurrency, live-content
`revision` schedules seasons/events/rewards, and `rulesetVersion` identifies immutable gameplay
semantics. Live-content publication must never change a balance manifest.

## Persistence and fail-closed rules

Migration `202608180008_balance_rulesets.sql` creates the append-only `balance_rulesets` catalog.
Database triggers reject update or deletion. `game_rooms` and `game_results` reference the catalog
with the composite `(ruleset_version, balance_checksum)` key; results additionally retain the full
manifest. This prevents a version number from being silently reused for different numbers.

Pre-catalog snapshots deserialize specifically as V1. The fallback is deliberately `v1()`, never
`current()`, so a future V2 binary cannot reinterpret an old record. An active room must use an
engine-registered pin byte-for-byte and fails before mutation, persistence, or snapshot delivery
otherwise. A completed room may remain readable from its integrity-checked embedded manifest; this
lets a newer finished ruleset remain interpretable by an older operational tool that understands
manifest schema 1 but must not execute it.

The restore verifier checks catalog manifests, checksums, indexed room pins, embedded room/game
pins, result manifests, and room/result agreement. Redis is only a cache of the already pinned room.

## Publishing a balance change

1. Add a new immutable `BalanceManifest::vN()` and registry entry. Never edit or remove an older
   constructor or checksum test.
2. Add an additive migration that inserts the exact manifest/checksum as version N. Never update an
   existing `balance_rulesets` row.
3. Add deterministic engine tests for both the old and new rulesets, including placement, turn,
   timeout, salvo, result, serialization, replay, and unknown-version refusal.
4. Run migration and restore verification before the server canary. Deploy ruleset-aware servers
   before web assets that expect the new manifest.
5. Drain or finish active rooms before setting N as current. Do not let a binary that cannot execute
   N own an N room; active unknown versions fail closed by design.
6. Run the complete PostgreSQL/Redis, browser, accessibility, artifact-budget, and production
   gameplay performance gates. Record both ruleset checksums in the release evidence.

A rollback deploys the prior immutable artifact and stops creating new-version rooms. It never
rewrites the catalog, result rows, or replay manifests. If new-version rooms are still active, keep
compatible owners running until those rooms finish or are explicitly cancelled by the incident
procedure.

## Verification coverage

- Domain tests lock the V1 checksum, reject tampering, refuse unregistered execution, and prove
  pre-catalog JSON becomes V1 rather than the latest version.
- API tests prove snapshots, history, and replay return the same pin with no client override.
- The real PostgreSQL/Redis suite proves catalog immutability, composite-key enforcement, result
  pinning, history/replay preservation, all prior distributed workflows, and restore verification.
- The full-game multi-browser test completes a match, renders the verified balance record and exact
  checksum in replay, and confirms the history entry retains `RULESET V1` without horizontal
  overflow.
