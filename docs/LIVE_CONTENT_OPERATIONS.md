# Live content operations

Mk.01 seasons, events, player-visible feature flags, and progression reward tuning use an
append-only revision ledger. PostgreSQL is authoritative in production; the memory implementation
has the same compare-and-swap behavior for local development and tests. The built-in revision `0`
is a safe fallback when no database revision has become active.

Revisions are operational release evidence, contain no player identity, and are retained
indefinitely. Corrections and rollback always append a new row; automated retention never deletes or
rewrites this audit ledger.

## Safety contract

Every candidate is rejected unless all of these conditions hold:

- `expectedRevision` equals the latest stored revision. Two operators cannot both publish from the
  same base revision.
- `activateAt` is no more than five minutes in the past or 90 days in the future. A scheduled
  revision is invisible until that UTC timestamp.
- Season IDs and event IDs are stable uppercase identifiers. A season lasts 7–200 days, contains at
  most 12 uniquely named events, and each event remains inside the season for at most 45 days.
- Titles, descriptions, operator identity, and the mandatory 8–256 character change note are
  bounded and reject control characters.
- Daily deployment XP stays between 25–500, daily accuracy XP between 25–750, and weekly supremacy
  XP between 100–2,500.
- Unknown JSON fields are rejected. The schema version is stored with each immutable payload.

`missionsEnabled` is the reward emergency stop. Turning it off removes claimable missions but does
not delete results or previously issued ledger entries. `eventBannerEnabled` hides active/upcoming
event promotion without changing progression. Claimed mission cards keep the amount actually
written to the reward ledger even if later tuning changes.

Live content does not mutate active room rules, game snapshots, or replay interpretation. Gameplay
balance changes still require a new ruleset version and the separate compatibility process in
`BALANCE_VERSIONING.md`.

## Review, validation, and publish

Prepare a JSON file containing the payload object accepted by `LiveContentPayload`. Use UTC RFC 3339
timestamps. Retrieve the latest immutable history first:

```sh
ADMIN_TOKEN_FILE=/run/secrets/mk01_admin_token \
OPERATOR_ID=liveops.alex \
MK01_BASE_URL=https://game.example.com \
npm run content:ops -- history --limit 10
```

Have a second operator review IDs, UTC windows, copy, flags, reward deltas, and the change note. Run
the server-side dry run with the observed revision:

```sh
ADMIN_TOKEN_FILE=/run/secrets/mk01_admin_token \
OPERATOR_ID=liveops.alex \
MK01_BASE_URL=https://game.example.com \
npm run content:ops -- validate release/live-content.json --expected 17
```

Only a response with `valid: true`, no issues, and candidate revision `18` is eligible. Publishing
reruns that exact validation before the atomic write and additionally requires `--confirm`:

```sh
ADMIN_TOKEN_FILE=/run/secrets/mk01_admin_token \
OPERATOR_ID=liveops.alex \
MK01_BASE_URL=https://game.example.com \
npm run content:ops -- publish release/live-content.json --expected 17 --confirm
```

Do not retry a conflict blindly. Fetch history, compare the intervening revision, merge deliberately,
and repeat review. The CLI never prints the bearer token and accepts the production token from the
managed secret file contract.

## Rollback and incident response

Rollback copies a prior payload into a new immediately active revision. It never changes or deletes
history, and its `rolledBackFromRevision` identifies the restored source. Revision `0` selects the
built-in safe baseline even though that fallback is not stored as a PostgreSQL row:

```sh
ADMIN_TOKEN_FILE=/run/secrets/mk01_admin_token \
OPERATOR_ID=incident.commander \
MK01_BASE_URL=https://game.example.com \
npm run content:ops -- rollback --expected 18 --target 17 \
  --note "Rollback reward tuning after duplicate economy alert" --confirm
```

An expired season cannot be restored because validation requires the season to remain valid at the
new activation time. For an expired target, publish a reviewed successor payload with corrected
dates instead. During a reward-integrity incident, first publish the mission kill switch, verify the
public `/api/content/live` revision and profile mission state, then follow the economy ledger and
incident-response runbooks.

## Verification and observability

- `mk01_live_content_published_total` counts successful validated publishes.
- `mk01_live_content_rollbacks_total` counts successful rollback revisions.
- The public endpoint returns only the active revision and omits ended events. The authenticated
  history endpoint returns the append-only operator, change note, activation, and rollback audit.
- PostgreSQL integration races two application instances against the same expected revision and
  proves exactly one commits. It also proves scheduled activation and cross-instance history.
- API integration covers authorization, dry run, out-of-range rejection, stale revision conflict,
  live reward application, both kill switches, rollback, and immutable history.

After publish, compare both counters and product API error rate, then inspect a player profile in the
target environment. A revision conflict is expected operator concurrency; storage errors, malformed
persisted payloads, or a public revision that differs across instances are incidents.
