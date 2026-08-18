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
- [x] Anti-cheat telemetry detects impossible order, automation, collusion, and intentional stalling.
- Evidence: player safety controls filter live and retained chat, block future room/matchmaking
  pairing in either direction, and capture an authoritative evidence window. The token-protected
  operator console/API supports searchable queues and append-only actions; suspensions/bans close
  live connections and gate later login/authentication, while reversal restores access. API tests
  exercise reporting, unauthorized admin access, search, suspension, enforcement, reversal, warning,
  and audit history. Conduct, evasion, escalation, retention, and appeal rules are defined in
  `docs/COMMUNITY_SAFETY.md`. Server-authoritative command rejection, WebSocket burst limits,
  repeated short-match pairing, and authoritative timeout counts emit deduplicated integrity
  signals with confidence, severity, evidence, occurrence count, Prometheus counters, and a private
  operator search queue. Tests distinguish retryable version races from impossible orders and
  exercise signal deduplication, private API filtering, third-match collusion, and stalling.

### B3. Application and supply-chain security

- [x] CSP, HSTS, frame protection, content sniffing, referrer, and permissions policies are verified.
- [x] Secrets come from a managed secret store and never from committed/default production values.
- [x] Dependency, license, secret, SAST, container, and infrastructure scans gate releases.
- [x] Threat models cover account, room, WebSocket, matchmaking, moderation, and admin surfaces.
- [x] A vulnerability response runbook, ownership, severity model, and patch SLO are exercised.

### B4. Data lifecycle

- [x] Sessions, abandoned rooms, chat, replay, telemetry, moderation, and account data have retention
      and deletion policies.
- [ ] User export and deletion cover every datastore, cache, backup policy, and derived record.
- [ ] PostgreSQL backups are encrypted and automated; restore drills meet RPO/RTO.
- [ ] Migrations are backward compatible with mixed-version rolling deployments.
- Evidence: `DATA_LIFECYCLE.md` defines enforced boundaries for every data class. The hourly worker
  prunes inactive sessions, abandoned matchmaking, terminal room/chat/replay records, closed
  moderation cases, and integrity signals using configurable UTC cutoffs and publishes per-class
  counters. Unit coverage proves expired terminal data is removed while active sessions and open or
  recent safety records survive. Account export/deletion APIs cover credentials, sessions,
  results, progression, social, moderation, integrity and both cache layers. An encrypted deletion
  ledger and fail-closed restore replay now prevent older backups from resurrecting an account; the
  gate remains open until the PostgreSQL CI restore artifact proves that replay end to end.

## Gate C — Game product and player retention

### C1. First-time experience and practice

- [x] Interactive tutorial teaches placement, targeting, turn timer, fog of war, and rematch.
- [x] Practice AI offers documented difficulty levels and deterministic test fixtures.
- [x] Contextual help and input prompts work on mouse, keyboard, touch, and controller if supported.
- [x] New-player funnel metrics identify failure and abandonment points.
- Evidence: the shell detects coarse touch, Pointer Events, and navigation keys and retains the most
  recent modality across route changes. Placement, targeting, and chat show live, contextual mouse,
  keyboard, or touch instructions; the E2E gate switches all three modalities in every context and
  verifies the exact actionable prompt. The accessibility and six-profile full-match regressions
  pass with the prompts rendered. Gamepad/controller input is intentionally not advertised or in
  the supported-input matrix for this web release. The aggregate-only funnel measures deduplicated
  reach, fixed-reason failure, and explicit/unload abandonment from first landing through tutorial,
  session, lobby, room, placement, first attack, and match completion. The server rejects arbitrary
  dimensions and identifiers; API integration and a real browser practice-match test prove every
  outcome class and the complete operational checkpoint path. `OPERATIONS.md` defines dashboard
  queries, denominators, release thresholds, and the interruption/recovery interpretation.

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

- [x] The complete flow, not only the landing page, is responsive on supported mobile/tablet/desktop
      classes.
- [x] Keyboard focus, dialogs, grids, chat, timers, errors, and live announcements pass WCAG 2.2 AA.
- [x] Color is never the only carrier of game state; color-vision presets are tested.
- [ ] All user-facing copy uses localization keys with Korean, English, and the launch locale set.
- [ ] Pseudolocalization, text expansion, locale dates/numbers, and font fallback are automated gates.
- Evidence: the two-client full-match suite asserts no horizontal document overflow at the lobby,
  waiting room, fleet placement, battle, refresh recovery, and result stages. It completes on
  Desktop Chrome, Desktop Firefox, Desktop Safari, Pixel 7, iPhone 13, and iPad Pro 11 profiles;
  the dedicated responsive landing smoke test also passes all six profiles. The axe-core 4.13
  gate audits WCAG 2.0/2.1/2.2 A and AA rules at landing, lobby, create/start/surrender dialogs,
  waiting, invitation, placement, both battle clients, chat/error, and both results. Its behavioral
  assertions prove modal focus trapping/restoration, roving grid focus and arrow/Space input,
  chat Enter/Escape input and focus restoration, scrollable log access, timer announcements, and
  alert/status live regions. Protanopia, deuteranopia, and tritanopia palettes persist per device,
  retain four distinct semantic colors, and pass the same WCAG rules in settings and live battle.
  Combat legends, coordinate labels, outcome-specific Wave/Flame shapes, text, selection, and
  status icons ensure that no state depends on color alone. The full Chromium suite and
  six-profile full-match regression pass.

### D4. Performance budgets

- [x] Route JS/CSS, fonts, images, audio, memory, CPU, animation frame time, and WebSocket bandwidth
      have budgets by device tier.
- [x] Korean fonts are subsetted and modern formats are preferred without redundant transfer.
- [x] Core Web Vitals and battle interaction latency are captured from real users by release.
- [x] Low-end mobile play remains readable and responsive during the heaviest effects sequence.
- Evidence: `check-font-subsets.mjs` scans all production Rust/Svelte/TypeScript copy, proves every
  static Korean glyph maps to one of 28 disjoint 400/700 WOFF2 slices, rejects duplicate selection
  and stale generated CSS, and caps the Korean payload at 450 KB. The production artifact fell from
  1,091,828 to 483,304 WOFF2 bytes (55.7%) while the unchanged JS/CSS/font budgets pass. The browser
  transfer test rejects full Korean faces, WOFF/TTF, duplicate requests, missing regular/bold faces,
  and a route font transfer above 500 KB. Dynamic player copy uses the documented system fallback.
  `performance-budgets.json` is the shared artifact/runtime authority for JavaScript, CSS, fonts,
  images, audio, heap, CPU tasks, long tasks, frame p95, and WebSocket bytes. A production-adapter
  Playwright journey exercises placement, five deterministic target-lock/hit sequences, carrier
  sinking, surrender modal, and result at desktop, 3× CPU mobile, and 6× CPU low-mobile tiers. The
  reference run passed 3/3 with zero long tasks; low mobile held 18.6 ms frame p95 under 6× CPU
  throttle while retaining a 14 px status heading, 24 px board-cell floor, 40 px fire-control floor,
  and no horizontal overflow. Its report attaches 360×640 sunk-carrier and unobscured result
  captures. Desktop frame p95 improved from 66.7 to 33.7 ms after redundant full-surface backdrop
  filters were removed. The browser now reports lifecycle LCP, maximum-session CLS, p98 INP, and
  request-matched attack-result latency into fixed route/device Prometheus histograms. No identity,
  URL parameter, device model, or request ID is accepted or retained. API integration rejects
  unknown/high-cardinality dimensions and out-of-range values; a real Chromium practice attack
  proves all four histograms increase. Operations defines p75/p95 targets, minimum sample counts,
  and canary stop thresholds. CI reruns both release-blocking gates. See `FONT_DELIVERY.md` and
  `PERFORMANCE_BUDGETS.md` for generation, measurement, and review contracts.

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
- [x] Full matches run on Chromium, Firefox, WebKit, and supported mobile profiles.
- [ ] Component, accessibility, visual-regression, property, fuzz, load, soak, and chaos suites have
      owned thresholds.
- [ ] Coverage reports identify untested behavior; targets are risk-based rather than cosmetic.
- Evidence: the service-backed integration suite applies embedded migrations, races revision-CAS
  writes, fences stale owners, completes distributed matchmaking atomically, fans events between
  two Redis-backed instances, recovers an active match through instance replacement, and proves
  PostgreSQL remains authoritative when the optional Redis cache cannot connect. This gate remains
  open until the corrected suite passes in CI with real PostgreSQL and Redis services.

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
