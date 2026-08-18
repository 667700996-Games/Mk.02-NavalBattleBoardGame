# Player Support Operations

Player support and trust-and-safety staff act through authenticated product APIs rather than direct
database access. `/admin/support` handles account recovery and compromised sessions;
`/admin/moderation` handles reports, integrity evidence, warnings, suspensions, bans, dismissals,
and reversals. Neither console stores the admin token in browser storage.

## Account support workflow

1. Verify the player using the approved support policy outside the game and obtain either the exact
   account UUID or exact handle. The API performs no prefix, fuzzy, or bulk enumeration.
2. Enter a named operator ID and the managed admin token. A lookup returns only account ID, handle,
   creation time, session ID/nickname/timestamps/current room, and prior support actions. Recovery
   keys, session tokens, token hashes, IP addresses, and device fingerprints are never returned.
3. Select one session for a lost device or all sessions for a suspected account compromise.
4. Record an 8–500 character verified reason and type the account handle exactly. The server checks
   the operator ID and reason again; browser confirmation is an additional guard, not authority.
5. The store deletes the selected account-owned sessions and writes the action in the same
   PostgreSQL transaction. A missing or foreign session fails without an audit success row.
6. Confirm the returned affected session IDs, notify the player through the approved support
   channel, and advise recovery-key rotation procedures. Never ask the player to send a recovery
   key through chat or a ticket.

`player_support_actions` stores a random action ID, account ID, operator ID, fixed action kind,
reason, target session when applicable, affected session IDs, and UTC timestamp. Update and direct
delete triggers reject history mutation. Account privacy deletion is the only cascade allowed to
remove those personal audit rows, and support actions are included in the player's data export
before deletion.

## Authorization and incident use

`ADMIN_TOKEN[_FILE]` must come from the environment secret store, contain at least 32 characters,
and be rotated after suspected exposure. Every mutating request also requires a nonempty
`X-Operator-Id`; shared or anonymous operator IDs are prohibited by operating policy. Production
access is restricted to the support/trust-and-safety role and audited at the edge.

Use the support console for authentication-session recovery only. Use the moderation console for
community or competitive-integrity decisions so its append-only warning/suspension/ban/reversal
model remains authoritative. Service outages, bulk account impact, hidden-state exposure, or a
compromised admin token enter `INCIDENT_RESPONSE.md`; they are not repaired with ad hoc SQL.

The memory/API test proves private exact lookup, missing-operator rejection, session invalidation,
and audit retrieval. The PostgreSQL/Redis suite additionally proves transactional deletion,
append-only mutation rejection, and privacy-cascade cleanup.
