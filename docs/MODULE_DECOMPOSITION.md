# Module Decomposition Gate

This document records the executable acceptance boundary for the AAA readiness item covering room
state, timers, chat, matchmaking, presentation, and orchestration. Moving lines is not sufficient:
each module has one named responsibility, its existing behavior remains covered, and CI prevents the
monoliths from returning.

## Service orchestration

`apps/server/src/app.rs` owns composition, shared `AppState`, and process-level coordination. It
delegates behavior to these sibling modules:

| Module                | Responsibility                                                  |
| --------------------- | --------------------------------------------------------------- |
| `app/accounts.rs`     | Identity, account/session, progression, and deletion use cases  |
| `app/connections.rs`  | Bounded connection hub and personalized delivery                |
| `app/live_content.rs` | Live-content activation and publication orchestration           |
| `app/matchmaking.rs`  | Durable matchmaking enqueue, poll, cancel, and room creation    |
| `app/metrics.rs`      | Bounded metrics, SLO distributions, and Prometheus rendering    |
| `app/rooms.rs`        | Room loading, mutation, persistence, and event fan-out          |
| `app/router.rs`       | HTTP router, middleware, readiness, and response classification |
| `app/safety.rs`       | Moderation, integrity signals, and enforcement orchestration    |
| `app/timers.rs`       | Durable turn/reconnect recovery and scheduled expiry            |

The architecture gate limits `app.rs` and each direct runtime sibling to 800 lines. The test module
is separate so production orchestration cannot grow invisibly behind inline test fixtures.

## Authoritative room domain

`domain/room.rs` remains the public room aggregate and core state machine. It delegates:

| Module               | Responsibility                                                                       |
| -------------------- | ------------------------------------------------------------------------------------ |
| `room/chat.rs`       | Typed chat validation, rate control, history, and system messages                    |
| `room/state.rs`      | Lobby state derivation, idempotency resolution caches, and revision bumps            |
| `room/timers.rs`     | Turn deadlines, reconnect state, and expiration/forfeit transitions                  |
| `room/projection.rs` | Personalized snapshots, summaries, safe replay, and balance-pin validation           |
| `room/tests.rs`      | Aggregate transition, privacy, idempotency, chat, timer, and replay regression tests |

All aggregate files are capped at 1,000 lines. The domain dependency rule still rejects transport,
application, protocol, and persistence imports, so decomposition cannot move authority outward.

## Browser presentation and orchestration

The lobby route owns session recovery, API calls, polling, navigation, and funnel reporting. It passes
state and callbacks to `LobbyCommandDashboard` and `LobbyRoomOperations`.
Those presentation components cannot import API, realtime, or global stores. Their stylesheet is
route-scoped under `.lobby-page`, so extraction does not leak presentation rules to other routes.

The room route owns recovery, realtime synchronization, navigation, and command dispatch. Waiting,
fleet placement, battle, result, chat, and disconnect presentation remain separate components.

All route components are capped at 1,200 total lines and 650 lines before an inline style block. The
lobby orchestration route has a stricter 400-line cap, and each lobby presentation component has a
250-line cap.

## Verification

Run the same gates locally that CI runs:

```sh
npm run architecture:check
npm run check
npm run lint
npm test
npm run build
npm run test:e2e
```

Acceptance requires the architecture gate, compile/type/lint checks, server/domain/API/contract unit
and integration tests, a production build, and the supported desktop/mobile browser matrix. A new
responsibility must be extracted or the reviewed threshold intentionally changed; it cannot bypass
the gate by hiding generated or critical source files.
