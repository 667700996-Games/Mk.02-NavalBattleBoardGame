# Mk.01 Data Lifecycle Policy

This policy is enforced by the server retention worker and is part of the release contract. All
durations are measured from authoritative UTC timestamps. Production overrides must be approved,
documented, and no longer than the values below without a legal basis.

| Data class | Default retention | Enforcement and deletion boundary |
| --- | ---: | --- |
| Anonymous and authenticated sessions | 30 days inactive | Expired inactive rows and token hashes are deleted hourly. Active-room sessions remain until the room resolves. |
| Abandoned matchmaking entries | 10 minutes | Durable queue rows are deleted hourly and opportunistically during queue operations. |
| Cancelled and completed rooms | 90 days | The room snapshot, chat, replay timeline, result, and participant index are deleted together. Redis room cache entries are evicted in the same sweep. |
| Active rooms | Until resolved | Durable reconnect/turn deadlines resolve abandoned games; active state is never deleted solely because a process stopped. |
| Account identity | Until user deletion | Accounts survive session expiry so the recovery credential can issue a fresh session. |
| Operational metrics | Aggregated only | The application endpoint exposes counters and gauges without player identifiers. Infrastructure retention must not exceed 30 days. |
| Closed moderation cases | 365 days after closure | Reports and their action audit rows are removed together. Open/reviewing cases are retained until resolved; legal holds require a separately audited export before closure. |
| Integrity telemetry | 180 days after last observation | Deduplicated anti-cheat signals and evidence are removed by the same hourly worker. Aggregated counters contain no player identity. |
| Privacy request audit | Operational audit lifetime | Only a random request ID, one-way subject fingerprint, request type, status, and timestamps survive deletion; credentials and account IDs are never written. |
| Encrypted backups | 35 days | Provider lifecycle rules delete expired backup objects; a user deletion becomes absent when the last protected backup expires. |

`SESSION_TTL_SECONDS`, `MATCHMAKING_ENTRY_TTL_SECONDS`,
`COMPLETED_ROOM_RETENTION_SECONDS`, `RETENTION_SWEEP_INTERVAL_SECONDS`,
`MODERATION_RETENTION_SECONDS`, and `INTEGRITY_SIGNAL_RETENTION_SECONDS` configure the executable
worker. Lower values may be used in staging. Increasing production values requires privacy and
operations approval.

Game-result participant rows preserve the account-to-player mapping independently of device
sessions. This lets expired session tokens be deleted without breaking an account owner's retained
history. The participant index is deleted by the same database cascade as its room and result.

Deletion sweeps expose cumulative Prometheus counters for sessions, rooms, matchmaking rows,
closed moderation cases, and integrity signals.
Any sweep failure is an error log and alert condition. Quarterly restore drills validate that
retention does not corrupt active state and that expired data is absent from restored samples after
the backup window.

## Player export and deletion

An authenticated account can request `GET /api/accounts/export`. The server takes a consistent
snapshot and returns account profile, device sessions, game history, progression rewards, owned
social relationships, submitted/received moderation cases and actions, and integrity signals. The
archive explicitly excludes session-token hashes and the recovery credential. Redis is only a
cache/fan-out layer and has no independent account archive.

`DELETE /api/accounts` requires both the 256-bit recovery key and the exact confirmation phrase
`DELETE`. Before erasure, every account session is removed from matchmaking and any live room;
active games resolve through the normal authoritative forfeit/cancellation transition. The delete
transaction then:

- removes all account sessions and token hashes, progression rewards, social relationships,
  reports/actions, integrity signals, matchmaking rows, and the account credential;
- replaces participant session IDs, account mappings, player names, authored chat, and operation
  names in completed records with unlinkable placeholders while preserving aggregate match
  correctness;
- evicts every affected Redis and application room cache and closes all device connections; and
- records only the non-identifying privacy request audit described above.

Deletion is intentionally not applied in-place to encrypted immutable backups. Backup objects are
inaccessible to application traffic, expire within 35 days, and any restore procedure must replay
the externally retained deletion ledger before the database can receive traffic. A restore that
cannot prove this replay is destroyed and may not be promoted.
