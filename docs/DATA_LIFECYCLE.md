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
| Security and moderation evidence | 365 days after closure | Access is restricted to authorized operators; legal holds override automated expiry and must be audited. |
| Encrypted backups | 35 days | Provider lifecycle rules delete expired backup objects; a user deletion becomes absent when the last protected backup expires. |

`SESSION_TTL_SECONDS`, `MATCHMAKING_ENTRY_TTL_SECONDS`,
`COMPLETED_ROOM_RETENTION_SECONDS`, and `RETENTION_SWEEP_INTERVAL_SECONDS` configure the executable
worker. Lower values may be used in staging. Increasing production values requires privacy and
operations approval.

Game-result participant rows preserve the account-to-player mapping independently of device
sessions. This lets expired session tokens be deleted without breaking an account owner's retained
history. The participant index is deleted by the same database cascade as its room and result.

Deletion sweeps expose cumulative Prometheus counters for sessions, rooms, and matchmaking rows.
Any sweep failure is an error log and alert condition. Quarterly restore drills validate that
retention does not corrupt active state and that expired data is absent from restored samples after
the backup window.

