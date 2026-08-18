# Social operations contract

Mk.01's social graph is account-only. Guest play and the existing in-match mute, block, and report
controls remain available, but durable friends, parties, presence, recent players, and direct game
invites require an upgraded account. Social APIs never disclose session IDs, tokens, recovery
credentials, hidden boards, or private-room state.

## Relationship lifecycle

- Friend requests use an opaque request ID and mirrored `OUTGOING` / `INCOMING` states. Acceptance
  changes both rows to `FRIEND`; rejection or removal clears both sides.
- Parties are intentionally limited to two friends for this one-on-one release. A mirrored invite
  becomes `OWNER` / `MEMBER`; either member can dissolve it. A player cannot join a second party.
- A direct invite is available only to friends, points to a one-player waiting room, expires after
  15 minutes, and is mirrored as `OUTGOING` / `INCOMING`. Acceptance returns the room code only to
  the invited account; the normal authoritative join endpoint still validates capacity and blocks.
- Blocking either direction prevents social actions and clears friendship, party, and pending game
  invite state on both sides. Mute and block continue to use the match-safety relationship so chat
  filtering and matchmaking exclusion retain their existing behavior.

Pair updates are serialized in memory and committed together in one PostgreSQL transaction. The
additive `player_social_links` table is separate from the stable `player_relationships` mute/block
table, so an older server in a rolling deployment can continue writing its original columns.

## Presence, recent players, and privacy

Presence is calculated from authoritative account sessions: an assigned room is `IN_GAME`, a
session seen during the last five minutes is `ONLINE`, and otherwise it is `OFFLINE`. It is returned
only when both accounts are friends, neither side is blocked, and the target enables presence
sharing. The current room ID is never offered as a join credential.

Each account independently controls friend requests, friend-only presence, and direct game invites.
Recent players are the 20 most recently completed, unique, account-backed human opponents, ordered
by authoritative result time and annotated with the viewer's friend/mute/block state.

## Data lifecycle and acceptance evidence

Privacy export includes the three privacy choices, legacy safety relationships, and social links.
Deletion removes links in either direction before deleting the account; foreign keys also cascade
as defense in depth, and restore verification counts links against deletion tombstones.

The memory-backed API test covers guest rejection, privacy denial, request/accept, friend-only
presence, party acceptance, direct invite acceptance, and one-time join-code delivery. The existing
safety integration still covers mute, block, reports, and future pairing rejection. The browser
gate completes the same friend-to-room journey in Chromium, Firefox, and WebKit, audits WCAG 2.2 AA,
and rejects horizontal overflow. The additive 21st migration is enforced by the rolling-compatible
migration policy and the PostgreSQL privacy fixture covers export and bidirectional deletion.
