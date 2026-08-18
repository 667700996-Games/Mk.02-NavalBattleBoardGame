# Mk.01 Threat Model

## Security objectives and trust boundaries

The Rust service is the sole authority for identity, room membership, placement, turn order,
attacks, timers, outcomes, replay disclosure, and persistence. Browsers, WebSocket frames, HTTP
bodies, proxy headers, room codes, and Redis messages are untrusted. PostgreSQL is the durable
source of truth; Redis may distribute events and limits but may not authorize a game mutation.

Protected assets are session and recovery credentials, hidden fleets, match integrity, account
history, progression economy, live-content configuration, chat and report evidence, service
capacity, and operator access. The main boundaries are
browser → gateway, gateway → application, application → PostgreSQL/Redis, application instance →
instance, and deployment automation → production secrets.

## Abuse cases and controls

| Surface | Primary threats | Required controls and current evidence |
| --- | --- | --- |
| Account and session | token theft/fixation, account enumeration, recovery brute force, stale device | 256-bit server tokens and recovery keys; hash-only storage; HttpOnly/Secure/SameSite/scoped expiry; atomic rotation on upgrade; uniform failed login; remote session revocation; positive/negative API tests |
| Room and game | forged player IDs, hidden-state reads, invalid placement, duplicate/out-of-order fire, stale owner | session-to-player binding; personalized snapshots; server validation; request IDs and turn/revision checks; PostgreSQL CAS plus authority lease/fencing; unit and multi-instance tests |
| WebSocket | cross-site hijack, oversized frames, event flood, slow consumer, reconnect storm | origin allowlist or bearer auth; 64 KiB frame/message cap; shared per-session event limit; bounded send queues; connection cap; reconnect backoff and durable deadlines |
| Matchmaking | duplicate match, queue flooding, abandoned claim, cross-instance race | durable queue, row locks with `SKIP LOCKED`, atomic pair completion, claim expiry, queue capacity and age metrics, distributed integration tests |
| Chat | script injection, spam, control characters, impersonation | server message types and allowlists; text validation; bounded history; rate limits; text-only Svelte rendering; server-only system identity |
| Replay | premature fleet disclosure, ID leakage, incompatible interpretation | participant authorization; FINISHED-only disclosure; immutable ruleset manifest and SHA-256 catalog pin; no session IDs; deterministic timeline tests |
| Live content | stolen operator token, reward inflation, stale concurrent publish, unsafe schedule, audit deletion | managed admin secret and explicit operator ID; server-side allowlist/range/time validation; PostgreSQL advisory-lock CAS; append-only revisions; dry run and explicit CLI confirmation; immutable rollback revision; API and cross-instance tests |
| Dependencies and build | vulnerable or malicious package/action, secret commit, license breach, unsafe image/IaC | locked Rust/npm graphs; Dependency Review; cargo-deny/RustSec; CodeQL for Rust, JS/TS, and Actions; Trivy vulnerability/secret/license/misconfiguration scan; Dependabot |
| Deployment and secrets | development defaults in production, plaintext committed secret, insecure origin/cookie, partial rollout | production fail-closed validation; explicit PostgreSQL/Redis secret injection or mounted `_FILE`; HTTPS/Secure/coordination requirements; immutable-artifact and canary runbook |
| Operations | direct database mutation, leaked logs, backup exposure, destructive recovery | structured safe client errors; no credential logging; least-privilege operator roles; encrypted backup and isolated restore drill; audited runbook actions |

## Residual risk and review triggers

The in-process WebSocket connection ceiling must also be enforced at the gateway across replicas.
Recovery keys are possession credentials, so the UI requires one-time secure storage and remote
session review. Redis loss intentionally makes production readiness fail; PostgreSQL fencing still
prevents stale commits.

Review this model before adding a new client event, account credential, matchmaking constraint,
moderation/admin action, datastore, analytics identifier, payment/economy feature, third-party SDK,
or protocol-breaking release. Every review records the owner, date, changed data flow, abuse cases,
tests, monitoring, and accepted residual risks.
