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
- [x] Ranked matchmaking supports region, latency, rating, party, and widening search constraints.
- [x] Turn and reconnect deadlines are durable jobs claimed once with fencing/idempotency.
- [x] No process scans every active room at a fixed sub-second interval.
- Evidence: two-instance matchmaking tests, worker failover tests, timer duplicate-delivery tests,
  and queue-depth/age metrics. Ranked tickets use a measured, bounded RTT and player-selected region
  while PostgreSQL supplies rating, account-backed solo-party identity, queue age, and atomic claim
  authority. Exact, regional, and global windows widen only when both players satisfy the elapsed,
  rating, RTT, party, and region constraints. Domain/memory/API tests reject guests, injected
  rating/party fields, spoofed stored ratings, profile changes, and same-account self-matches. A
  real PostgreSQL/Redis two-instance test proves 31-second mutual widening and rolling-compatible
  legacy casual writes; Chromium, Firefox, and WebKit verify the lobby's RTT probe, safe request,
  visible search phase/rating, polling, and cancellation. `RANKED_MATCHMAKING.md` fixes the policy
  and explicitly leaves ranked progression and seasons to C2.

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
- [x] User export and deletion cover every datastore, cache, backup policy, and derived record.
- [x] PostgreSQL backups are encrypted and automated; restore drills meet RPO/RTO.
- [x] Migrations are backward compatible with mixed-version rolling deployments.
- Evidence: `DATA_LIFECYCLE.md` defines enforced boundaries for every data class. The hourly worker
  prunes inactive sessions, abandoned matchmaking, terminal room/chat/replay records, closed
  moderation cases, and integrity signals using configurable UTC cutoffs and publishes per-class
  counters. Unit coverage proves expired terminal data is removed while active sessions and open or
  recent safety records survive. Account export/deletion APIs cover credentials, current and
  expired-session history, progression, ranked standings/rewards/deltas, leaderboard entries,
  social, reports, direct moderation targets, integrity, room/result copies, and both cache layers.
  The real PostgreSQL/Redis test expires the last session, verifies every export class, anonymizes
  room and result identifiers, evicts Redis, removes all derived rows, and preserves unrelated
  evidence. The automated backup job uses a random per-run key, SHA-256 and GPG AES-256 for both the
  backup and independent deletion ledger, refuses stale/corrupt inputs and non-isolated targets,
  reapplies tombstones, runs migrations and full restore verification, enforces RPO/RTO, and retains
  90-day JSON evidence. The clean parity drill restored in one second and found zero resurrected
  personal records across the comprehensive fixture. Every database entry point now verifies all
  known checksums while tolerating a later additive migration already applied by a candidate. The
  real PostgreSQL compatibility case corrupts a known checksum and proves fail-closed behavior,
  restores it, adds an unknown future migration and nullable column, then proves stable startup,
  migrate-only, deletion-ledger replay, restore verification, and old/new SQL projections remain
  bidirectionally writable. A database dual-write trigger indexes results written with the original
  stable column set, so candidate history, account identity, and deletion paths cannot miss a match
  completed during the mixed window. The self-testing migration policy rejects destructive data,
  DDL, defaults, type, RLS, and permission changes and guarantees new migration files invalidate the
  embedded release build. The clean 12-case service run passed in 10.77 seconds, and the encrypted
  restore drill reapplied all 20 migrations with zero resurrected personal rows.

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
- [x] Ranked play has rating, placement matches, seasons, tiers, decay/inactivity policy, and rewards.
- [x] Match fairness considers rating, latency, region, rematches, and queue time.
- [x] Leaderboards are abuse-resistant, paginated, privacy-aware, and seasonally archived.
- [x] Balance changes are versioned so replays and historical results remain interpretable.
- Evidence: `RANKED_COMPETITION.md` fixes the five-match placement, expected-score rating, tier,
  soft-reset, 14-day/weekly decay, and XP reward policies. Match rooms pin their immutable live
  season; only active seasons accept queues, and season keys prevent cross-season pairing. The
  PostgreSQL result transaction locks both standings, inserts one room settlement marker, updates
  rating projections, and writes unique match/placement/season rewards atomically. Memory/domain/
  API tests plus a fresh 20-migration PostgreSQL 16 database and the thirteen-test PostgreSQL/Redis
  suite prove spoof resistance, idempotent settlement, five placements, rollover rewards, export,
  deletion, restore verification, and mixed-version queue behavior. The stats view exposes the
  active provisional/tier rating and multi-browser E2E covers the player flow.
- Evidence: ranked candidates satisfy both players' server-owned rating, RTT, region, season, solo
  party, block-list, and durable wait windows. Authoritative results identify opponents seen in the
  previous 30 minutes: both tickets must reach 90-second `GLOBAL` before a repeat is eligible,
  novel opponents retain priority through 179 seconds, and 180-second mutual wait restores FIFO to
  prevent starvation. Rooms persist only anonymous recent-pair count, relaxation, shared-wait, and
  skew evidence. Domain, memory, API, and real two-instance PostgreSQL/Redis tests prove mutual
  widening, immediate-repeat rejection, novel-candidate priority, eventual relaxation, and
  identity privacy. A metric, dashboard, 25%-with-volume ticket alert, runbook, and additive result
  indexes make pool diversity observable without player labels.
- Evidence: `RANKED_LEADERBOARDS.md` admits only placement-complete standings whose match total
  exactly matches authoritative seasonal settlement rows. Five-minute server snapshots and random
  stored cursors prevent rank-key injection and cross-page drift; a 24-hour settlement window then
  produces one immutable archive per past season. Public entries omit all IDs, authenticated players
  can opt out immediately, deletion cascades through snapshots, and active account/session penalties
  remain filtered until reversal. Memory, API, real PostgreSQL, privacy-export, restore, and
  multi-browser tests cover ordering, cursor bounds, archive immutability, opt-out, and moderation.
  Anonymous counters, a dashboard, volume-gated empty-board alert, and runbook cover live integrity.
- Evidence: `BALANCE_VERSIONING.md` defines an append-only gameplay catalog distinct from protocol,
  room, and live-content revisions. Each room pins the full board, fleet, shot, timer, timeout,
  victory, duplicate-target, and reveal manifest plus its SHA-256; the engine and adaptive web board
  consume that pin, while snapshots, history, results, and replay retain it. Pre-catalog JSON always
  becomes V1 rather than the latest default. Unknown or altered active pins fail before execution or
  persistence. Composite database foreign keys prevent version/checksum reuse, a trigger rejects
  catalog edits/deletes, and restore verification compares catalog, room, game, and result copies.
  Domain/API tests, a fresh 20-migration PostgreSQL 16 database, the thirteen-test PostgreSQL/Redis
  suite, and multi-browser full-match replay cover legacy recovery, tampering, immutable history,
  exact UI interpretation, and rollback-safe retention.

### C3. Progression and live content

- [x] Profile progression, achievements, daily/weekly missions, and meaningful non-pay-to-win rewards
      exist.
- [x] Seasons, events, content configuration, feature flags, and safe live tuning are supported.
- [ ] Cosmetics cover fleet, board, effects, profile, and presentation without leaking hidden state.
- [x] Economy and reward issuance are transactional, idempotent, auditable, and rollback-safe.
- Evidence: profile XP/level/rank and achievements are deterministic projections of the
  authoritative result ledger; daily/weekly mission rewards use a unique account/source/period
  ledger, exclude reversed rows, and have API and duplicate-claim tests. Live seasons, bounded
  events, mission/event kill switches, and reward tuning use immutable schema-versioned revisions,
  scheduled activation, compare-and-swap publication, dry-run validation, audited operators, and
  rollback-as-a-new-revision. The guarded CLI requires an explicit confirmation after validation;
  public/profile APIs and the responsive stats view expose only the active projection. Memory,
  API, accessibility, six-profile full-match, and real PostgreSQL 16/Redis multi-instance tests
  cover conflicting publishers, scheduled activation, kill switches, history, and rollback.

### C4. Social, spectating, and replay

- [ ] Friends, parties, direct invites, recent players, presence, privacy, mute, and block exist.
- [x] Spectators receive delayed, visibility-filtered authoritative state.
- [x] Deterministic replays include ruleset/protocol versions and cannot expose hidden information
      before a match is complete.
- [x] Post-match analysis can step through turns, compare decisions, and share a safe replay link.
- Evidence: the participant-only replay now derives both players' authoritative accuracy, opening/
  midgame/endgame splits, hit/miss streaks, sinks, timeouts, and hit-follow-up discipline. It ranks
  finishing attacks, sinks, momentum streaks and cumulative time pressure as up to three decisive
  moments that jump back to their timeline event, then emits sample-gated improvement tips. Copying
  the current replay URL explicitly preserves participant-session authorization; non-participants
  remain rejected by the server. Four deterministic analysis tests cover normal, low-accuracy,
  surrender and timeout endings, while the six-profile full-match E2E checks both cards, all phase
  meters, decisive-event navigation, the safe-link disclosure and horizontal overflow; Chromium
  profiles additionally prove the clipboard value exactly matches the participant replay URL.
- Evidence: public matches now expose a separate server-authored spectator projection with a fixed
  30-second horizon; private rooms return not-found and viewers never join the participant room
  channel. Attack and timeout events, active turn, finish phase, and result are all released against
  the same authoritative `visibleThrough` timestamp. The allowlist excludes boards, unhit fleets,
  placements, session IDs, reconnect state, chat, and tokens. Deterministic domain tests prove the
  29/30-second boundary and result delay, the authenticated HTTP test scans serialized output for
  hidden fields, and isolated host/guest/viewer E2E passes in Chromium, Firefox, and WebKit with two
  fleet-free responsive grids. `SPECTATING.md` fixes the privacy and timing contract.

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
- [x] All user-facing copy uses localization keys with Korean, English, and the launch locale set.
- [x] Pseudolocalization, text expansion, locale dates/numbers, and font fallback are automated gates.
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
- Evidence: `LOCALIZATION.md` owns the `ko-KR` default and `en-US` launch catalogs, the test-only
  `en-XA` pseudo locale, persistence and fallback behavior, error-code translation, and copy
  contribution contract. `npm run localization:check`, included in the release lint chain, rejects
  catalog or placeholder drift, HTML copy, English-catalog Korean, hard-coded Korean in shipped
  sources, literal user-facing Svelte copy, missing Intl formatters, pseudo-policy drift, and font
  fallback regression. Unit tests expand every catalog entry by at least 25%, preserve placeholders,
  prove Korean/English catalog parity, verify locale-specific dates, numbers and relative time, and
  translate structured server errors without exposing the server-language message. The dedicated
  Playwright gate switches `en-US` to `en-XA`, proves persisted locale/document metadata, measures
  at least 25% title expansion, and rejects body or document overflow on Chromium, Firefox, WebKit,
  Pixel 7, iPhone 13 and iPad Pro 11 profiles. Localized JSON participates in generated Korean font
  slicing; `npm run fonts:check` proves all glyphs are covered within the strictest 500 KB device
  font budget.

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

- [x] Large room and UI modules are split by state machine, timer, chat, matchmaking, presentation,
      and orchestration responsibility.
- [x] HTTP/WebSocket schemas generate or validate Rust and TypeScript contracts at runtime.
- [x] Protocol compatibility and migration policy supports mixed client/server release windows.
- [x] Architecture decisions and ownership boundaries are documented and reviewed.
- Evidence: `MODULE_DECOMPOSITION.md` maps each server, room-domain, and lobby/room UI responsibility
  to a bounded module. The former application monolith is now an orchestration shell plus account,
  connection, live-content, matchmaking, metrics, room, router, safety, and timer modules; no runtime
  service module exceeds 800 lines. The room domain is a 759-line state-machine core with separate
  chat, projection, state-helper, timer/recovery, and test modules, all below the enforced 1,000-line
  ceiling. The lobby route fell from 1,443 to fewer than 400 lines and delegates matchmaking plus
  room-browser/modal presentation to callback-driven components that cannot import API,
  realtime, or global-state modules. The room route continues to delegate waiting, placement,
  battle, result, and chat responsibilities. `architecture:check` now requires all of these modules,
  enforces server/room/route/component limits and presentation dependency direction, and assigns all
  290 critical files to exactly one owner boundary. Full Rust/web checks, lint, 104 Rust tests, 36 web
  unit tests, a warning-free production build, and the multi-browser/mobile E2E release matrix pass.
- Evidence: `PROTOCOL_COMPATIBILITY.md` fixes explicit HTTP/WebSocket negotiation, the frozen
  headerless V2 fallback, current-plus-one-prior support, server-first rollout, rollback, a minimum
  30-day window, seven zero-traffic days, and active-match drain before retirement. Every API
  response advertises the selected/range/capability contract; unsupported explicit clients fail
  with 426 before gameplay state changes. The web accepts a newer server only when V2 remains in
  its advertised range and retains the frozen missing-header/empty-subprotocol fallback for a
  stable pre-negotiation V2 server. Checksummed manifests and per-version client fixtures are
  immutable; the contract gate requires artifacts for every supported version and the Rust test
  deserializes every frozen command. API and real loopback WebSocket integration tests prove old
  headerless V2, explicit V2 selection, unsupported/malformed rejection, and bounded version
  metrics. A 13th Grafana panel plus a volume-gated rejection alert enforce the canary gate. The
  complete Rust/web checks, lint, 104 Rust tests, 36 web unit tests, production build, and 33 executed
  multi-browser/mobile E2E cases passed; all six full-game profiles completed and recovered after
  refresh with the negotiated V2 socket.
- Evidence: `ARCHITECTURE.md` records the authority/dependency map, runtime flows, seven path
  boundaries, review classes, and the responsibility-based decomposition sequence. Three accepted
  ADRs fix server authority, PostgreSQL authority with Redis coordination, and independent immutable
  protocol/schema/balance/content history. The dated baseline review covers every decision and
  boundary, passes authority/dependency/durability/compatibility/ownership review, and keeps
  `ARCH-001`/`ARCH-002` open with owners and measurable acceptance instead of hiding decomposition
  debt. The machine-readable ownership policy maps 290 critical files to exactly one boundary and
  six roles; CODEOWNERS uses a verified GitHub account, while the PR template requires boundary,
  owner, ADR, compatibility, rollback, and independent high-risk review evidence. The executable
  gate rejects uncovered/overlapping ownership, missing ADR/review/CODEOWNERS data, and forbidden
  domain→transport/store, store→transport, or pure browser-game→network/global-state dependencies.
  CI runs it explicitly and through lint; architecture, full check/lint, 104 Rust tests, and 36 web
  unit tests passed.

### E2. Automated quality gates

- [x] CI runs formatting, linting, type checks, unit, integration, contract, E2E, build, audit, and
      artifact checks on every change.
- [x] PostgreSQL and Redis integration tests exercise migrations, recovery, cache failure, and
      concurrent writes.
- [x] Full matches run on Chromium, Firefox, WebKit, and supported mobile profiles.
- [x] Component, accessibility, visual-regression, property, fuzz, load, soak, and chaos suites have
      owned thresholds.
- [x] Coverage reports identify untested behavior; targets are risk-based rather than cosmetic.
- Evidence: the service-backed integration suite applies embedded migrations, races revision-CAS
  writes, fences stale owners, completes distributed matchmaking atomically, fans events between
  two Redis-backed instances, recovers an active match through instance replacement, and proves
  PostgreSQL remains authoritative when the optional Redis cache cannot connect. The dedicated CI
  job uses health-checked PostgreSQL 16 and Redis 7 containers, requires both URLs so no test can
  silently skip, serializes the thirteen shared-database cases, and blocks browser and backup jobs.
  Its post-suite restore verifier checks all 20 migrations and retained snapshots, then uploads a
  90-day JSON evidence artifact. `DISTRIBUTED_INTEGRATION.md` fixes the local reproduction, covered
  boundaries, acceptance rules, and failure triage.
- Evidence: `config/quality-gates.json` assigns component, WCAG 2.2 AA, desktop/mobile golden,
  deterministic property, seeded libFuzzer, authenticated HTTP/WebSocket load, weekly 30-minute
  soak, and PostgreSQL-reset chaos suites to five registered roles with executable failure
  thresholds. The policy checker verifies all eight sources, commands, owners, goldens, corpus
  seeds, CI schedules, and values. Four state-focused Svelte component tests, 4,096 generated
  placement cases, 256 complete attack permutations, and four fixed-time visual goldens execute in
  the normal portfolio. PR CI adds a 20-second sanitizer fuzz run, eight-VU p95/p99 load gate, and
  Toxiproxy drill that requires readiness failure, live liveness, exact snapshot recovery, and a
  15-second RTO; the scheduled workflow adds a ten-minute fuzz campaign and 30-minute soak. Local
  references completed 1,000 fuzz inputs, 797 authenticated load journeys with 3,993/3,993 checks,
  and an exact-snapshot PostgreSQL recovery in 2.107 seconds.
- Evidence: Vitest instruments six behavior-bearing protocol, RUM, placement, replay, and extracted
  lobby files and enforces both aggregate and file-specific floors; the measured reference is
  94.93% statements, 90.88% branches, 91.76% functions, and 97.19% lines. Rust source coverage
  enforces 60% aggregate line/function floors plus eleven
  higher-risk file targets, with the service-backed CI job refusing to skip PostgreSQL/Redis cases.
  The first baseline measured 65.34% lines and 65.49% functions without those services, while core
  protocol, balance, board, game, matchmaking, room, rate-limit, API/router/safety, and memory-store
  floors all passed. PostgreSQL projection and WebSocket lifetime gaps remain explicitly listed
  with owners, compensating distributed/browser/load/fuzz suites, and review dates. See
  `QUALITY_GATES.md` for the exact policy and triage contract.

### E3. Release and service operations

- [x] Separate development, staging, canary, and production environments use reproducible artifacts.
- [x] Deployments include preflight, migration safety, canary analysis, rollback, and active-match
      compatibility gates.
- [x] SLOs cover availability, matchmaking latency, command latency, disconnect rate, and recovery.
- [x] Dashboards, alerts, incident roles, status communication, runbooks, and postmortems exist.
- [x] Customer support and moderation tooling can act without direct database access.
- Evidence: a protected release-build workflow performs one locked server/web OCI build, emits SPDX
  SBOMs and GitHub/Sigstore provenance, keyless-signs both image digests, and binds the digests to
  the Git SHA, commit epoch, migrations, contracts, balance source, and lockfiles in a hashed
  manifest. Four machine-readable environments form an immutable development→staging→canary→
  production promotion chain. The executable gate rejects rebuilt artifacts, missing or stale
  evidence, non-additive migrations, incompatible active snapshots, insufficient samples, SLO or
  protocol failures, duplicate/missing approvals, stale backup/restore evidence, and an unproven
  rollback. Deployment runs migrate-only before replacement and confirms gateway readiness and V2;
  rollback restores the exact prior application digest within 15 minutes without reversing data.
  Five release-gate tests exercise the successful chain and fail-closed threshold/source-drift paths.
- Evidence: `/admin/support` performs authenticated exact UUID/handle lookup without credential,
  token, IP, or device disclosure; a named operator can revoke one or all account sessions only
  after supplying a bounded reason and exact-handle UI confirmation. The PostgreSQL operation locks
  the account, deletes only owned sessions, and inserts an append-only audit row in one transaction;
  direct history edits fail while account privacy deletion cascades the personal audit. Memory API
  and service-backed PostgreSQL tests cover authorization, lookup, revocation, audit retrieval,
  immutable history, and deletion. The existing `/admin/moderation` console remains the audited
  warning, suspension, ban, dismissal, reversal, report, and integrity-signal path.
- Evidence: product API response-class counters exclude probes, metrics and telemetry; shared
  HTTP/WebSocket command histograms distinguish accepted and rejected work; durable matchmaking
  records both players' queue-to-room latency; socket totals, abnormal closes and connected exposure
  form an auditable disconnect denominator; and active-match recovery starts from the persisted
  disconnect boundary and ends after the authoritative save. Unit and API integration tests verify bounded
  Prometheus output, route exclusions, paired wait samples and the reconnect sample. `OPERATIONS.md`
  fixes the objectives, PromQL, minimum samples, and multi-window availability burn-rate gate.
- Evidence: the provisioned Grafana dashboard contains thirteen stable SLO/player-experience panels;
  fourteen Prometheus alerts route pages and tickets through versioned Alertmanager policy; and every
  alert links to a concrete operations runbook anchor. `INCIDENT_RESPONSE.md` defines severity,
  five owned roles, escalation, evidence, communication cadence and closure. Public status and
  blameless postmortem templates require next-update clocks, confirmed player impact, causal
  analysis and owner/due-date/verification actions. `npm run observability:check` rejects missing or
  identity-labelled signals, panels, alerts, roles, templates and routing; CI also runs official
  `promtool check rules` and `amtool check-config` against the deployable files.

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
