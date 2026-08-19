# Protocol Compatibility and Release Windows

Mk.01 treats the public HTTP and WebSocket contract as a release artifact. Stable and candidate
application versions may share a pool only while every client contract in the advertised protocol
window remains accepted and every active match can reconnect without changing its interpretation.
Database compatibility is governed separately by `MIGRATIONS.md`; both gates must pass for the same
release.

## Current baseline and invariants

- The current and headerless legacy baseline is V3. HTTP uses `x-mk01-protocol-version: 3` and
  WebSocket uses `Sec-WebSocket-Protocol: mk01.v3` when the client can negotiate explicitly.
- Frozen V2 artifacts remain checksummed as historical evidence, but V2 is no longer in the active
  compatibility window after removal of the explicit rematch command and snapshot state.
- `GET /api/protocol` reports `currentVersion`, the inclusive minimum/maximum range, the headerless
  default, the minimum compatibility-window days, and bounded capability identifiers.
- Every `/api/*` response, including a protocol rejection, reports the selected, minimum, maximum,
  and capability headers. Unsupported or malformed explicit HTTP versions fail with HTTP 426 and
  `SERVER_PROTOCOL_MISMATCH`; unsupported WebSocket offers fail the upgrade with HTTP 426.
- A cached client from before explicit negotiation is mapped only to the oldest supported frozen
  version. Missing negotiation is not a request for the newest behavior.
- At most the current and one immediately preceding protocol are served together. The oldest
  supported version remains the headerless default until it is retired.
- The advertised compatibility period is at least 30 days. Removing an old protocol additionally
  requires zero active matches pinned to it and seven consecutive days with no accepted production
  traffic for that version. The time and drain conditions are cumulative, not alternatives.
- Version and outcome labels are fixed by the supported range. Player, session, room, request, build,
  or arbitrary client identifiers never enter protocol metrics.

## Required mixed-release matrix

| Client                    | Server                               | Expected result                                                                                                                                                                                          |
| ------------------------- | ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Stable pre-negotiation V3 | Candidate supporting V3              | Headerless HTTP and WebSocket select the frozen V3 behavior; the WebSocket response may omit a subprotocol.                                                                                              |
| Current V3                | Stable pre-negotiation V3            | Missing response headers, a 404 from `/api/protocol`, and an empty selected WebSocket subprotocol use the frozen V3 fallback. Snapshot and event runtime validation still reject anything other than V3. |
| Current V3                | Candidate supporting V3              | HTTP selects V3 and WebSocket echoes `mk01.v3`; capabilities come only from bounded server metadata.                                                                                                     |
| V3                        | Future V4 server advertising `[3,4]` | Explicit V3 remains selected even though the descriptor's current version is V4. The V3 serializer/deserializer and active-match interpretation remain pinned.                                           |
| Unsupported or malformed  | Any release                          | HTTP/WebSocket handshake fails with 426 before authentication state or gameplay state changes.                                                                                                           |

An empty negotiation response is accepted only because V3 is the deployed frozen contract
before negotiation was introduced. No later protocol may create a second implicit fallback.

## Change classification

Changes that preserve the frozen version may add an optional response field that old readers are
proven to ignore or an independent HTTP route. Existing client command payloads use
`deny_unknown_fields`; adding a request field to one of those commands is therefore breaking even if
the field is called optional. A new WebSocket event or advertised capability also changes the
checksummed manifest and requires a new version.

A version bump is required for removed or renamed fields/events, changed enum representations,
different units or meanings, new required request or response data, hidden-state visibility changes,
or any snapshot/rules interpretation that the frozen client cannot preserve. A database or balance
ruleset revision does not replace a protocol bump.

Files already recorded in `contracts/checksums.sha256` are immutable. Correct a mistake by adding a
new protocol version; never edit or delete a frozen manifest or fixture. `npm run contract` proves:

1. Rust and TypeScript agree on current/minimum/maximum/default/window constants, capabilities, and
   every event name;
2. every version in the supported range has a manifest;
3. every frozen artifact still matches its SHA-256 ledger entry; and
4. every supported version has one fixture for each of its frozen client events and all of those
   fixtures still deserialize through the candidate Rust server.

When V4 is introduced, first add V4 artifacts and keep V3 fixtures. The server's connection entry is
already pinned to its negotiated version; any breaking server event must add an explicit
per-connection V3 adapter on both local and distributed delivery paths before `maximum` becomes 4.
The stable V3 web artifact must be run against that candidate output. Merely storing the negotiated
number is not evidence that an adapter exists.

## Release, rollback, and retirement

1. Freeze the new manifest and fixtures, add their checksums, and pass contract, Rust, web unit,
   HTTP integration, actual WebSocket handshake, distributed-service, and supported-browser tests.
2. Deploy a server that accepts both versions before publishing a client that emits the new one.
   Keep the old serializer/deserializer and old active-match snapshot path enabled.
3. In staging, run the complete matrix above, including a match started on stable, candidate server
   replacement, reconnect, one further authoritative command, and result/replay verification.
4. Canary at 10%, 25%, 50%, then 100%. Stop on any unsupported negotiation from a supported release,
   snapshot/event decode error, unexpected disconnect regression, stale room commit, or SLO burn.
5. Roll back the web artifact first when client negotiation fails; roll back the server artifact when
   it cannot preserve the old contract. Do not contract the database or delete an adapter during the
   rollback window.
6. Retire the oldest version only after the minimum 30 days, seven zero-traffic days, no active match
   or reconnect record pinned to it, and an approved release record. Retirement is a later release
   that advances `minimum` and `legacyDefault` together and retains the frozen artifacts/checksums.

`mk01_protocol_negotiations_total{transport,version,outcome}` is the operational proof. During a
canary, graph
`sum by (transport,version,outcome) (increase(mk01_protocol_negotiations_total[15m]))` and calculate
rejections over all negotiations. Any rejection attributable to a supported release blocks
promotion. The general alert opens when rejection share exceeds 1% with at least 100 negotiations;
operators then split by transport, inspect deploy annotations and sampled edge logs, and reproduce
the matrix without adding identity labels.

## Local acceptance commands

```sh
npm run contract
cargo test -p mk01-server protocol::tests::every_supported_frozen_client_remains_accepted_during_the_release_window -- --exact
cargo test -p mk01-server --test api_flow protocol_window_accepts_headerless_v3_and_rejects_unsupported_clients -- --exact
cargo test -p mk01-server --test api_flow websocket_handshake_supports_headerless_and_explicit_v3_clients -- --exact
npm --workspace @mk01/web run test:unit -- src/lib/protocol.test.ts
npm run test:distributed
npm run test:e2e
```

The WebSocket integration case binds a loopback listener, so a restricted development sandbox may
need explicit permission. Skipping it is not equivalent evidence.
