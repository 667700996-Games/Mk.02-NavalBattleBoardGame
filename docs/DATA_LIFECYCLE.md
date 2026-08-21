# Mk.01 Data Lifecycle Policy

This policy is enforced by the server retention worker and is part of the release contract. All
durations are measured from authoritative UTC timestamps. Production overrides must be approved,
documented, and no longer than the values below without a legal basis.

| Data class                              |                          Default retention | Enforcement and deletion boundary                                                                                                                                                                                                                                                                                                                                  |
| --------------------------------------- | -----------------------------------------: | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Anonymous and authenticated sessions    |                           30 days inactive | Expired inactive rows and token hashes are deleted hourly. Active-room sessions remain until the room resolves.                                                                                                                                                                                                                                                    |
| Abandoned matchmaking entries           |                                 10 minutes | Durable queue rows are deleted hourly and opportunistically during queue operations.                                                                                                                                                                                                                                                                               |
| Cancelled and completed rooms           |                                    90 days | The room snapshot, chat, replay timeline, result, and participant index are deleted together. Redis room cache entries are evicted in the same sweep.                                                                                                                                                                                                              |
| Active rooms                            |                             Until resolved | Durable reconnect/turn deadlines resolve abandoned games; active state is never deleted solely because a process stopped.                                                                                                                                                                                                                                          |
| Account identity and ranked competition |                        Until user deletion | Current rating, seasonal standings, result deltas, rewards, leaderboard preference, and snapshot entries survive session expiry. Account-bound rows cascade with verified deletion; the room settlement marker and empty snapshot shell contain no identity.                                                                                                       |
| Operational metrics                     |                            Aggregated only | The application endpoint exposes counters, gauges, and histograms without player identifiers. Funnel and real-user performance input accepts only fixed enums and bounded numeric values; it never accepts an account, session, room, request ID, IP, nickname, URL parameter, device model, or free-text label. Infrastructure retention must not exceed 30 days. |
| Closed moderation cases                 |                     365 days after closure | Reports and their action audit rows are removed together. Open/reviewing cases are retained until resolved; legal holds require a separately audited export before closure.                                                                                                                                                                                        |
| Account support actions                 |                        Until user deletion | Named-operator session-revocation actions are append-only and included in account export. Account privacy deletion cascades these personal audit rows; aggregate operational counts must not retain the account ID.                                                                                                                                                |
| Integrity telemetry                     |            180 days after last observation | Deduplicated anti-cheat signals and evidence are removed by the same hourly worker. Aggregated counters contain no player identity.                                                                                                                                                                                                                                |
| Privacy request audit                   |                 Operational audit lifetime | Only a random request ID, one-way subject fingerprint, request type, status, and timestamps survive deletion; credentials and account IDs are never written.                                                                                                                                                                                                       |
| Privacy deletion tombstones             | Backup lifetime + 7 days (42 days minimum) | A random account UUID, request ID, one-way subject fingerprint and deletion time are kept in a separately encrypted deletion ledger solely to prevent resurrection from an older backup. Ledger-object lifecycle deletion is audited after every possibly older backup has expired.                                                                                |
| Encrypted backups                       |                                    35 days | Provider lifecycle rules delete expired backup objects. Every restore reapplies the latest independent deletion ledger before verification or traffic.                                                                                                                                                                                                             |

`SESSION_TTL_SECONDS`, `MATCHMAKING_ENTRY_TTL_SECONDS`,
`COMPLETED_ROOM_RETENTION_SECONDS`, `RETENTION_SWEEP_INTERVAL_SECONDS`,
`MODERATION_RETENTION_SECONDS`, and `INTEGRITY_SIGNAL_RETENTION_SECONDS` configure the executable
worker. Lower values may be used in staging. Increasing production values requires privacy and
operations approval.

Game-result participant rows preserve the account-to-player mapping independently of device
sessions. This lets expired session tokens be deleted without breaking an account owner's retained
history. Ranked matchmaking derives a 30-minute recent-opponent count from these retained rows and
stores no separate opponent profile or identity in metrics. The participant index is deleted by
the same database cascade as its room and result.

Leaderboard snapshots store account UUIDs only as private foreign keys; public responses join the
current handle and never serialize that key. Visibility and active moderation are evaluated on each
read. Deletion cascades every snapshot entry immediately, while an empty season snapshot can remain
as non-personal archive metadata. Active snapshots expire after five minutes and their opaque cursor
rows cascade; finalized past-season snapshots remain to preserve historical ranks.

Deletion sweeps expose cumulative Prometheus counters for sessions, rooms, matchmaking rows,
closed moderation cases, and integrity signals.
Any sweep failure is an error log and alert condition. Quarterly restore drills validate that
retention does not corrupt active state and that expired data is absent from restored samples after
the backup window.

New-player funnel counters are aggregate process metrics only. Browser session-storage keys contain
checkpoint names and deduplication flags, not identity or gameplay data, and expire with the tab's
session-storage lifetime. Because no subject identifier exists in the application metric, export or
account deletion has no individual funnel record to retrieve or erase. Prometheus/storage systems
must apply the 30-day operational-metric maximum above.

Real-user performance samples are converted immediately into fixed route/device histogram buckets.
The server stores no event row or browser identifier, and the client keeps an attack request ID only
in an in-memory timer map until its matching result or a 60-second expiry. Export and account
deletion therefore have no individual RUM record to retrieve or erase. Metrics infrastructure must
not enrich these histograms from access logs, fingerprints, or account/session data.

## Player export and deletion

An authenticated account can request `GET /api/accounts/export`. The server takes a consistent
snapshot and returns account profile, device sessions, game history, progression rewards, current
ranked rating, seasonal standings, per-match deltas, ranked rewards, active or archived leaderboard
snapshot entries, owned safety relationships, submitted/received moderation cases, actions whose
case or direct target belongs to the account, and integrity signals. Historical participant session
IDs are recovered from the durable result index, so session expiry does not make history, safety,
room, moderation, or support identities disappear from the export boundary. The archive explicitly excludes
session-token hashes and the recovery credential. Redis is only a cache/fan-out layer and has no
independent account archive.

`DELETE /api/accounts` requires both the 256-bit recovery key and the exact confirmation phrase
`DELETE`. Before erasure, every account session is removed from matchmaking and any live room;
active games resolve through the normal authoritative forfeit/cancellation transition. The delete
transaction then:

- removes all account sessions and token hashes, progression and ranked rewards, current rating,
  seasonal standings and ranked participant deltas, safety relationships,
  reports/actions, support actions, integrity signals, matchmaking rows, and the account credential;
- replaces participant session IDs, account mappings, player names, authored chat, and operation
  names in completed records with unlinkable placeholders while preserving aggregate match
  correctness;
- removes the account UUID from every result array even when all device sessions expired, deletes
  direct-target moderation actions independently of their report ownership, and cascades ratings,
  standings, rewards, ranked deltas, and leaderboard entries while retaining non-personal season
  snapshot shells;
- evicts every affected Redis and application room cache and closes all device connections; and
- records the non-identifying privacy request audit plus the minimal backup-deletion tombstone
  described above.

Deletion is not applied in-place to encrypted immutable backups. `scripts/export-deletion-ledger.sh`
exports the current tombstones as a checksum-protected GPG AES-256 object stored independently from
database backups. `scripts/restore-postgres-drill.sh` requires that ledger, verifies and decrypts it,
rejects a ledger older than the configured freshness gate (one hour by default), replays every
deletion against the isolated restored database, and rejects the restore if any
tombstoned account, session, reward, participant identity, or result identity remains. The replay is
idempotent and writes its counts into restore evidence. A restored copy may not receive traffic if
the current ledger is missing, stale, corrupt, or cannot be fully applied; it must be destroyed and
recovered from a valid backup/ledger pair.

The resurrection verifier enumerates accounts, sessions, progression and ranked rewards, ratings,
standings, ranked match and leaderboard participants, result participant rows and UUID arrays,
relationships, reports, direct moderation targets, and integrity subjects. The August 18, 2026
service-backed test expires the only device session before deletion, then proves the historical
result index still drives room/result anonymization, every derived export class is present, the
Redis room cache is evicted, unrelated moderation evidence survives, and the verifier reports zero
remaining personal records.
