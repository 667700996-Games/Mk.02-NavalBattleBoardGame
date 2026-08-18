# ADR-0001: Server-authoritative gameplay and personalized projections

- Status: Accepted
- Date: 2026-08-18
- Decision owner: `game-integrity`
- Reviewer roles: `architecture`, `service-platform`, `web-experience`
- Last reviewed: 2026-08-18

## Context

Naval battle depends on hidden fleet placement, strict turn order, pinned balance rules, deadlines,
and outcomes that remain trustworthy across reconnects and competing clients. Allowing the browser to
decide any of these makes cheating, desynchronization, and replay reinterpretation unavoidable.

## Decision

The Rust domain is the sole authority for room/game state, placement, attacks, timers, matchmaking,
rating, rewards, moderation enforcement, and replay facts. Clients send intent plus idempotency and
expected-revision evidence. The server authenticates the session-to-player binding, executes a domain
transition, commits it, and emits a projection filtered for that viewer. Opponent placement and any
unearned hidden information never enter a pre-finish projection.

The browser may predict presentation and validate convenience input, but server rejection always wins.
Replays and results retain their protocol and balance interpretation pins.

## Rejected alternatives

- Client-authoritative placement or hit calculation: trivial to forge and impossible to audit.
- Shared full snapshots with CSS-only hiding: leaks opponent state to the browser.
- Last-write-wins commands: permits stale tabs or paused owners to overwrite a newer turn.

## Consequences

Every gameplay feature needs domain behavior and server tests before UI affordances. Personalized
serialization is part of security review. Offline multiplayer is not supported by the online authority
model. More server work is accepted in exchange for deterministic recovery, anti-cheat evidence, and
stable replay semantics.

## Verification

Domain/property-style unit cases cover placement, turn, duplicate, timeout, replay, and hidden-state
rules. API, two-browser, distributed PostgreSQL/Redis, rolling-replacement, and replay tests verify that
clients cannot advance or observe state outside the authoritative projection.

## Review triggers

Review on a new game mode with different visibility, spectator support, offline play, client
prediction that mutates shared state, replay sharing changes, or any move of rule logic outside the
domain boundary.
