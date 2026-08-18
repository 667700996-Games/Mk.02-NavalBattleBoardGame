# Automated quality portfolio

`config/quality-gates.json` is the executable ownership and threshold registry for Mk.01 quality
tests. A threshold change needs the named owner's review and measured evidence; a failing candidate
must not raise a limit merely to become green. `npm run quality-gates:check` rejects a missing suite,
source, command, owner, threshold, golden, corpus seed, risk target, known-gap record, or CI schedule.

## Portfolio and failure contracts

| Suite             | Owner               | Pull-request gate                             | Failure threshold                                                                                |
| ----------------- | ------------------- | --------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Component         | Web experience      | Four lobby presentation-state tests           | Any behavior test fails or the minimum drops below four                                          |
| Accessibility     | Web experience      | WCAG 2.2 AA browser flow                      | Any axe violation across at least 12 audited states                                              |
| Visual regression | Web experience      | Four approved desktop/mobile Chromium goldens | More than 1% changed pixels at a 0.2 pixel threshold                                             |
| Property          | Game integrity      | 4,096 placements, 256 attack permutations     | Any boundary, uniqueness, hit-accounting, or terminal-state invariant fails                      |
| Fuzz              | Service platform    | 20-second seeded libFuzzer run                | Any crash, sanitizer finding, or two-second input timeout                                        |
| Load              | Service platform    | Eight-VU authenticated HTTP/WebSocket journey | Failure rate ≥1%, p95 ≥250 ms, or p99 ≥600 ms                                                    |
| Soak              | Release engineering | Weekly four-VU, 30-minute journey             | Failure rate ≥1% or p95 ≥250 ms                                                                  |
| Chaos             | Data reliability    | PostgreSQL connection reset through Toxiproxy | Readiness does not fail closed, liveness fails, snapshot is lost, or recovery exceeds 15 seconds |

The load server disables its in-process rate limits only for this isolated capacity journey so the
test measures application behavior rather than the separately tested abuse policy. Every virtual
user creates an independent cookie session, polls rooms/recovery/live content, upgrades an
authenticated V2 WebSocket, and requires a heartbeat acknowledgement. The resulting k6 JSON is an
artifact, and the wrapper rechecks every named threshold instead of trusting process exit alone.

The chaos drill first persists a private room through PostgreSQL. It disables only the PostgreSQL
proxy, requires `/api/ready` to stop reporting ready while `/api/health` stays live, restores the
proxy, measures recovery, and reloads the exact room ID/code/name. A trap always re-enables the
proxy. CI retains the structured evidence and server log for 90 days.

## Risk-based coverage

`npm run test:coverage:web` instruments only behavior-bearing client boundaries: protocol
compatibility, RUM delivery, fleet placement, replay analysis, and the extracted lobby components.
The total floor is 85% statements, 78% branches, 75% functions, and 87% lines. Each file has a
separate threshold based on its failure impact; for example protocol and replay lines require at
least 92% and 95%, while lobby callbacks have a lower function floor because SSR behavior tests do
not execute browser event handlers. Vitest emits text, LCOV, and JSON summaries and enforces the
same values itself.

`npm run test:coverage:rust` runs all Rust tests serially under `cargo-llvm-cov` and emits a JSON
summary. CI supplies mandatory PostgreSQL and Redis URLs, so the twelve service-backed tests cannot
silently skip. The registry requires at least 60% total line and function coverage plus individual
floors for the API/router/safety boundary, immutable balance, board/game/matchmaking/room rules,
protocol parsing, rate limiting, and the memory authority implementation. A global number alone
cannot compensate for a missed critical file.

The report intentionally records two risk gaps rather than hiding them:

- `store/postgres.rs` contains a large SQL projection surface that is skipped in service-free local
  coverage. The CI coverage job runs it with real PostgreSQL/Redis; the separate distributed suite
  remains the acceptance gate for transaction, fencing, recovery, and migration behavior.
- `ws.rs` lifetime and browser reconnection paths cross the process/browser boundary. Six full-match
  profiles, the authenticated k6 heartbeat, protocol fuzzing, and loopback handshake tests are the
  compensating suites until instrumented browser/server coverage is practical.

Both gaps have an owner, compensating suite, and September 17, 2026 review date in the registry.
New gaps require the same fields; removing a gap requires a report proving the behavior is now
measured.

The August 18, 2026 web reference passed at 94.93% statements, 90.88% branches, 91.76%
functions, and 97.19% lines. The service-free Rust reference passed at 65.34% lines and 65.49%
functions before CI adds real PostgreSQL/Redis execution. The same-day eight-VU load reference
completed 797 authenticated HTTP/WebSocket journeys with 3,993/3,993 checks, zero request or
workflow failures, and a 2.25 ms critical-API p95 against the 250 ms limit. The PostgreSQL-reset
chaos reference failed readiness closed, kept liveness available, recovered the exact persisted
room snapshot in 2.107 seconds, and remained within the 15-second recovery budget.

## Execution cadence

```bash
npm run quality-gates:check  # registry, ownership, thresholds, sources, goldens, CI wiring
npm run test:component      # extracted Svelte responsibility states
npm run test:property       # deterministic generated Rust invariants
npm run test:coverage:web   # Vitest risk coverage
npm run test:coverage:rust  # cargo-llvm-cov JSON; service URLs strengthen this locally
npm run test:visual         # approved desktop/mobile goldens
npm run test:fuzz           # 20-second protocol boundary run; Rust nightly + cargo-fuzz
npm run test:load           # k6 directly or pinned Docker fallback
npm run test:soak           # 30 minutes by default
npm run test:chaos          # expects configured Toxiproxy/PostgreSQL/Redis and server
```

PR CI runs the short fuzz, load, visual, coverage, and chaos gates in dedicated jobs. The scheduled
workflow runs a ten-minute fuzz campaign and the 30-minute soak every Monday. Failure artifacts are
retained for 30 days on PRs and 90 days for scheduled or chaos evidence.

For a failure, preserve the first report and do not immediately retry. Reproduce the named command,
separate an infrastructure-startup failure from a product threshold breach, and compare the exact
metric or golden. Visual changes need explicit art-direction approval and regenerated goldens;
coverage changes need a test or a documented risk-gap review; load/soak/chaos changes need a
measured cause and remediation before any policy adjustment.
