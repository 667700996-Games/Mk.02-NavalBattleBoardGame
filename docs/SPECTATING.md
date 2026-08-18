# Delayed spectating policy

Mk.01 exposes spectating as a server-authored public record, never as a copy of either player's
personalized game snapshot. Only authenticated sessions can enumerate or open feeds, and only rooms
created with `PUBLIC` visibility become spectatable. Private-room lookups return the same not-found
response as an unknown room so their existence is not disclosed.

## Delay and visibility contract

The server owns a fixed 30-second release horizon. Every response includes `serverTimestamp`,
`visibleThrough`, and `delaySeconds`; clients render that contract and cannot request a shorter
delay. Timeline attacks and turn-expiration records are included only when their authoritative
timestamp is at or before `visibleThrough`. A result and `FINISHED` phase are withheld until the
result timestamp crosses the same horizon, preventing the winner or finish reason from revealing a
still-buffered action.

The spectator projection is an allowlist containing room metadata, public player IDs and names,
the immutable balance/rules pin, delayed attack and timeout events, the delay-derived active player,
and a delay-cleared result. It never serializes boards, ship placements, unhit fleet cells, pending
placements, participant session IDs, reconnection deadlines, chat, tokens, or personalized replay
data. Sunk ship class remains visible only as part of an already released attack result, matching
the information available on a participant's target board.

The browser polls the projection rather than joining the room WebSocket hub. It renders two strike
grids derived only from released attacks, shows an explicit delay/security badge and visibility
timestamp, and provides no gameplay or chat controls.

## Acceptance evidence

- The domain test advances a deterministic clock across attack and result horizons, proves the
  29-second view remains hidden and the 30-second view becomes visible, rejects private rooms, and
  scans serialized output for hidden fields.
- The API integration test requires authentication, lists only public active/finished battles,
  verifies the fixed delay, and repeats the serialization leak scan at the HTTP boundary.
- `e2e/spectating.spec.ts` creates a public battle with isolated host, guest, and viewer sessions in
  Chromium, Firefox, and WebKit. It verifies delayed lobby discovery, response allowlisting, two
  fleet-free grids, the explicit 30-second notice, and horizontal viewport containment.

Any new spectator field must be added to this allowlist, reviewed for timing inference, covered by
both serialization tests, and remain compatible with the frozen gameplay protocol window.
