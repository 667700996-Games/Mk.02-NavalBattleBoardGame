# Mk.01 AAA Readiness Program

This document is the authoritative implementation and acceptance checklist for turning Mk.01
from a polished two-player vertical slice into an AAA-quality online strategy game. An item is not
complete because code exists: its acceptance evidence must also be present and passing.

## Definition of done

The program is complete only when all gates below are satisfied in a production-like environment.

- Product: a new player can learn, play, progress, compete, socialize, and return without relying
  on an external guide.
- Gameplay: casual, ranked, practice, and configurable matches are server authoritative, balanced,
  replayable, and protected against abuse.
- Platform: the service survives instance replacement, horizontal scaling, dependency failures,
  slow clients, reconnect storms, and rolling deployment without corrupting a match.
- Presentation: gameplay has production art, animation, VFX, music, layered SFX, responsive input,
  and measurable performance budgets on supported devices.
- Quality: CI, contract, unit, integration, end-to-end, accessibility, visual, load, soak, fuzz, and
  failure-injection gates cover every supported release target.
- Operations: SLOs, dashboards, alerts, runbooks, moderation, data retention, backup restore,
  staged rollout, and rollback are exercised rather than documented only.
- Accessibility and reach: the complete game meets WCAG 2.2 AA, supports keyboard and screen-reader
  play, color-vision alternatives, reduced motion, and every shipped locale.

## Gate A — Authoritative online platform

### A1. Distributed room authority

- [x] A room has exactly one active authoritative owner at a time.
- [x] Ownership is leased or fenced so a paused/stale process cannot commit after takeover.
- [x] Every mutation uses a persistence revision and rejects stale writes atomically.
- [x] WebSocket fan-out reaches players connected to different application instances.
- [x] Instance termination transfers or recovers active rooms within the reconnect SLO.
- [x] A rolling deployment preserves active matches across protocol-compatible releases.
- Evidence: multi-instance integration test, stale-owner test, rolling-restart test, persisted event
  or revision audit, and a production-like architecture diagram. The rolling-instance PostgreSQL/
  Redis test reconnects a player, preserves game ID/turn/protocol/privacy, commits the next attack,
  and asserts recovery inside the configured reconnect SLO.

### A2. Matchmaking and timers

- [x] Matchmaking is durable, distributed, idempotent, cancellable, and cleans abandoned entries.
- [ ] Ranked matchmaking supports region, latency, rating, party, and widening search constraints.
- [x] Turn and reconnect deadlines are durable jobs claimed once with fencing/idempotency.
- [x] No process scans every active room at a fixed sub-second interval.
- Evidence: two-instance matchmaking tests, worker failover tests, timer duplicate-delivery tests,
  and queue-depth/age metrics.

### A3. Transport protection and backpressure

- [x] WebSocket outbound queues are bounded and disconnect slow consumers with an observable reason.
- [x] HTTP, session creation, WebSocket connection, and WebSocket event limits exist per IP/session.
- [x] Limits are enforced through shared infrastructure when multiple instances are deployed.
- [x] Payload, frame, connection, room, matchmaking, and retention quotas are configured.
- [x] Retry behavior uses explicit retryability and backoff contracts.
- Evidence: abusive-client tests, slow-consumer tests, reconnect-storm load test, and rate-limit
  metrics split by endpoint/reason. HTTP/WebSocket payloads are capped at 64 KiB, connection send
  queues are bounded, room and matchmaking admission has tested capacity limits, room chat/history
  is bounded, and the executable retention worker publishes deletion metrics.

## Gate B — Security, identity, safety, and data

### B1. Identity and sessions

- [x] Guest play remains available, with account upgrade that preserves history and identity.
- [x] Accounts support verified sign-in, logout, session listing/revocation, and secure recovery.
- [x] Session cookies are HttpOnly, Secure in production, SameSite, scoped, rotated, and expirable.
- [x] Authentication and authorization have positive and negative integration coverage.
- Evidence: account migration and hashed 256-bit recovery credentials; atomic guest-upgrade/session-
  token rotation; cross-session history lookup; API integration tests for valid/invalid login, stale
  token rejection, session listing/revocation, one-time recovery disclosure, and production cookie
  attributes.

### B2. Abuse prevention and moderation

- [x] Players can mute, block, and report chat, names, and gameplay behavior.
- [x] Operators can search evidence, warn, suspend, ban, reverse actions, and audit every action.
- [x] Name/chat policy, spam controls, evasion handling, and appeal workflow are defined.
- [ ] Anti-cheat telemetry detects impossible order, automation, collusion, and intentional stalling.
- Evidence: player safety controls filter live and retained chat, block future room/matchmaking
  pairing in either direction, and capture an authoritative evidence window. The token-protected
  operator console/API supports searchable queues and append-only actions; suspensions/bans close
  live connections and gate later login/authentication, while reversal restores access. API tests
  exercise reporting, unauthorized admin access, search, suspension, enforcement, reversal, warning,
  and audit history. Conduct, evasion, escalation, retention, and appeal rules are defined in
  `docs/COMMUNITY_SAFETY.md`.

### B3. Application and supply-chain security

- [x] CSP, HSTS, frame protection, content sniffing, referrer, and permissions policies are verified.
- [x] Secrets come from a managed secret store and never from committed/default production values.
- [x] Dependency, license, secret, SAST, container, and infrastructure scans gate releases.
- [x] Threat models cover account, room, WebSocket, matchmaking, moderation, and admin surfaces.
- [ ] A vulnerability response runbook, ownership, severity model, and patch SLO are exercised.

### B4. Data lifecycle

- [ ] Sessions, abandoned rooms, chat, replay, telemetry, moderation, and account data have retention
      and deletion policies.
- [ ] User export and deletion cover every datastore, cache, backup policy, and derived record.
- [ ] PostgreSQL backups are encrypted and automated; restore drills meet RPO/RTO.
- [ ] Migrations are backward compatible with mixed-version rolling deployments.

## Gate C — Game product and player retention

### C1. First-time experience and practice

- [x] Interactive tutorial teaches placement, targeting, turn timer, fog of war, and rematch.
- [x] Practice AI offers documented difficulty levels and deterministic test fixtures.
- [ ] Contextual help and input prompts work on mouse, keyboard, touch, and controller if supported.
- [ ] New-player funnel metrics identify failure and abandonment points.

### C2. Modes and competition

- [x] Casual classic mode is joined by rapid, salvo, and configurable private rules.
- [ ] Ranked play has rating, placement matches, seasons, tiers, decay/inactivity policy, and rewards.
- [ ] Match fairness considers rating, latency, region, rematches, and queue time.
- [ ] Leaderboards are abuse-resistant, paginated, privacy-aware, and seasonally archived.
- [ ] Balance changes are versioned so replays and historical results remain interpretable.

### C3. Progression and live content

- [x] Profile progression, achievements, daily/weekly missions, and meaningful non-pay-to-win rewards
      exist.
- [ ] Seasons, events, content configuration, feature flags, and safe live tuning are supported.
- [ ] Cosmetics cover fleet, board, effects, profile, and presentation without leaking hidden state.
- [x] Economy and reward issuance are transactional, idempotent, auditable, and rollback-safe.
- Evidence: profile XP/level/rank and achievements are deterministic projections of the
  authoritative result ledger; daily/weekly mission rewards use a unique account/source/period
  ledger, exclude reversed rows, and have API and duplicate-claim tests.

### C4. Social, spectating, and replay

- [ ] Friends, parties, direct invites, recent players, presence, privacy, mute, and block exist.
- [ ] Spectators receive delayed, visibility-filtered authoritative state.
- [x] Deterministic replays include ruleset/protocol versions and cannot expose hidden information
      before a match is complete.
- [ ] Post-match analysis can step through turns, compare decisions, and share a safe replay link.

## Gate D — Presentation and experience

### D1. Art, animation, and VFX

- [ ] A production art bible defines shape, color, motion, typography, readability, and asset tiers.
- [ ] Ships, ocean, targeting, hits, misses, sinking, victory, defeat, and transitions use final art.
- [ ] Effects preserve board readability and have reduced-motion and low-performance alternatives.
- [ ] Visual quality is verified through approved golden captures across supported viewports.

### D2. Audio and haptics

- [ ] Final music, ambience, UI, weapon, impact, sinking, victory, and defeat assets replace prototype
      oscillator tones.
- [ ] Music, effects, ambience, voice, and master volume are independently adjustable.
- [ ] Audio handles focus, backgrounding, interruptions, device changes, and accessibility cues.
- [ ] Supported mobile devices receive intentional, optional haptic feedback.

### D3. UX, accessibility, localization

- [ ] The complete flow, not only the landing page, is responsive on supported mobile/tablet/desktop
      classes.
- [ ] Keyboard focus, dialogs, grids, chat, timers, errors, and live announcements pass WCAG 2.2 AA.
- [ ] Color is never the only carrier of game state; color-vision presets are tested.
- [ ] All user-facing copy uses localization keys with Korean, English, and the launch locale set.
- [ ] Pseudolocalization, text expansion, locale dates/numbers, and font fallback are automated gates.

### D4. Performance budgets

- [ ] Route JS/CSS, fonts, images, audio, memory, CPU, animation frame time, and WebSocket bandwidth
      have budgets by device tier.
- [ ] Korean fonts are subsetted and modern formats are preferred without redundant transfer.
- [ ] Core Web Vitals and battle interaction latency are captured from real users by release.
- [ ] Low-end mobile play remains readable and responsive during the heaviest effects sequence.

## Gate E — Engineering quality and delivery

### E1. Maintainable architecture and contracts

- [ ] Large room and UI modules are split by state machine, timer, chat, matchmaking, presentation,
      and orchestration responsibility.
- [x] HTTP/WebSocket schemas generate or validate Rust and TypeScript contracts at runtime.
- [ ] Protocol compatibility and migration policy supports mixed client/server release windows.
- [ ] Architecture decisions and ownership boundaries are documented and reviewed.

### E2. Automated quality gates

- [x] CI runs formatting, linting, type checks, unit, integration, contract, E2E, build, audit, and
      artifact checks on every change.
- [ ] PostgreSQL and Redis integration tests exercise migrations, recovery, cache failure, and
      concurrent writes.
- [ ] Full matches run on Chromium, Firefox, WebKit, and supported mobile profiles.
- [ ] Component, accessibility, visual-regression, property, fuzz, load, soak, and chaos suites have
      owned thresholds.
- [ ] Coverage reports identify untested behavior; targets are risk-based rather than cosmetic.

### E3. Release and service operations

- [ ] Separate development, staging, canary, and production environments use reproducible artifacts.
- [ ] Deployments include preflight, migration safety, canary analysis, rollback, and active-match
      compatibility gates.
- [ ] SLOs cover availability, matchmaking latency, command latency, disconnect rate, and recovery.
- [ ] Dashboards, alerts, incident roles, status communication, runbooks, and postmortems exist.
- [ ] Customer support and moderation tooling can act without direct database access.

## Delivery order

1. Platform safety: bounded queues, rate limits, readiness, security headers, session revocation.
2. Distributed correctness: fenced room ownership, atomic revision writes, distributed events,
   matchmaking, and deadline workers.
3. Engineering gates: generated contracts, module boundaries, CI expansion, database/browser/load
   tests, observability, backup and deployment drills.
4. Product foundation: account upgrade, tutorial, AI practice, modes, ranked, replay, social safety.
5. Production experience: final art/audio/VFX, performance tiers, accessibility, localization.
6. Live service: progression, seasons, cosmetics, content operations, support, staged launch.

The ordering expresses dependencies, not reduced scope. Every gate remains required for the AAA
objective.
