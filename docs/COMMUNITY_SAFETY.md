# Mk.01 Community Safety and Moderation Policy

This policy governs player names, room chat, tactical signals, reports, enforcement, evasion, and
appeals. It is a release requirement, not optional guidance. The Trust & Safety on-call owns the
queue and the Security Incident Commander owns credible threats to accounts or service integrity.

## Player conduct

Names and messages must not contain harassment, threats, hate directed at a protected class,
sexual content involving minors, doxxing, impersonation, fraud, malicious links, or instructions
to evade enforcement. Spam includes repeated messages, disruptive signal flooding, unsolicited
advertising, and automated input. Gameplay abuse includes cheating, automation, collusion,
intentional stalling, match fixing, exploiting hidden information, and knowingly reproducing an
unreported integrity flaw.

The server rejects markup and control characters, caps messages at 300 characters, deduplicates
client message IDs, limits WebSocket events per session, and bounds retained room chat. Players can
mute or block an opponent immediately. A block suppresses communication and excludes both players
from direct room joins and matchmaking pair selection in either direction.

## Report evidence and triage

A report contains the category, reporter and target stable identities, room/player identifiers,
protocol and room versions, room state, the recent authoritative chat and attack window, the
player's explanation, and UTC capture time. Reports are immutable evidence; operator decisions are
append-only actions. Operators search and review them at `/admin/moderation`, authenticated with a
managed `ADMIN_TOKEN` and a named `X-Operator-Id`.

| Priority | Examples | First review target |
| --- | --- | ---: |
| P0 | Credible imminent physical harm, child safety, active account compromise | 15 minutes |
| P1 | Hate/threat campaigns, active cheats at scale, doxxing | 2 hours |
| P2 | Harassment, automation, collusion, repeated stalling | 24 hours |
| P3 | Spam, inappropriate name, isolated low-impact conduct | 3 business days |

Operators must record one of `WARN`, time-bounded `SUSPEND`, `BAN`, or `DISMISS`, with a concrete
reason. A `REVERSE` action must reference the exact prior action, preserves the original record,
and returns the case to review. Suspensions and bans close active sockets and are checked during
every HTTP/WebSocket authentication and account login. No moderator should act on a case involving
their own account or a close collaborator; reassign it and record the conflict.

## Proportional enforcement

- Warning: first low-impact name, chat, spam, or etiquette violation.
- 1–72 hour suspension: repeated low-impact conduct, disruptive stalling, or credible automation
  pending investigation.
- 7–30 day suspension: serious harassment, repeat automation/collusion, or deliberate exploitation.
- Permanent ban: severe threats, doxxing, child-safety violations, commercial cheating, repeated
  evasion, or conduct creating material risk to players or the service.
- Dismissal: insufficient evidence, mistaken target, protected good-faith security research, or
  conduct outside policy.

## Evasion and linked identity handling

Account sanctions apply to every active session on that account and to prior guest-session aliases
retained under it. A newly upgraded account cannot shed a sanction placed on its guest identity.
Signals such as repeated device/network identifiers may be used only as risk evidence, never as the
sole basis for permanent action. Operators link related cases, avoid revealing detection logic, and
escalate coordinated abuse to Security. Creating more accounts after a restriction is an
aggravating factor and may extend the restriction.

## Appeals

The player submits the account ID, action date, and concise reason through the private security
contact documented in `SECURITY.md`, with subject `MODERATION APPEAL`; secrets and recovery keys
must never be included. A different operator reviews the original evidence, policy version,
proportionality, and relevant linked cases. Appeals receive acknowledgment within 3 business days
and a decision target of 10 business days. Accepted appeals append a `REVERSE` action; records are
never edited or silently deleted. One appeal is accepted per action unless materially new evidence
exists.

## Privacy, retention, and quality review

Moderation evidence is least-privilege, access-logged operational data retained for 365 days after
case closure unless a documented legal hold applies. Quarterly calibration samples reversed,
dismissed, and upheld cases for consistency and bias. Monthly metrics cover queue age, decision
time, reversal rate, repeated offenders, spam rejection, and integrity-signal precision without
publishing player identifiers. See `DATA_LIFECYCLE.md` for deletion and backup boundaries.

## Game-integrity detection

The integrity pipeline records evidence for four independent detectors without automatically
punishing a player: authoritative impossible-order/identity violations, sustained WebSocket event
bursts, three or more short surrender/disconnect matches by the same pair in seven days, and three
or more authoritative turn timeouts. Signals include protocol/game/room context, severity,
confidence, occurrence count, and UTC timestamps. Legitimate persistence revision conflicts and
storage failures are explicitly excluded from the impossible-order detector. Operators correlate a
signal with reports and replay evidence before enforcement; detector output alone cannot produce a
permanent ban.
