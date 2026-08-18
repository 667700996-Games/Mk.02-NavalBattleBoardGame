# Mk.01 Blameless Incident Postmortem

- Incident ID: `INC-YYYY-NNN`
- Severity: `SEV-1 | SEV-2 | SEV-3`
- Started / detected / mitigated / resolved (UTC):
- Incident commander / operations lead / game-integrity lead / communications lead / scribe:
- Affected artifact SHA, environment, regions, modes, and protocol versions:
- Review date and reviewers:

## Executive summary

Explain what players experienced, how long it lasted, why it happened, and how service was restored.
Use system and process language; this is a blameless analysis, not an attribution exercise.

## Impact

Quantify affected requests, sessions, matches, queue waits, disconnects, recovery time, results,
rewards, rankings, replays, accounts, and data where evidence permits. Separate confirmed impact,
maximum possible scope, and disproved hypotheses. State whether hidden information, winner
correctness, duplicate rewards, personal data, or deletion guarantees were affected.

## Detection and response

Identify the first signal, why the relevant dashboard/alert did or did not work, time to acknowledge,
time to mitigate, update cadence adherence, handoffs, and whether the rollback/recovery runbook was
accurate.

## Timeline

Use ordered UTC timestamps. Record alerts, deployments, hypotheses, decisions, mitigations, public
updates, validation, and closure. Link sanitized evidence for every decisive claim.

| UTC time | Role | Observation or decision | Evidence |
| --- | --- | --- | --- |
| `YYYY-MM-DDTHH:MM:SSZ` | role | event | restricted link |

## Root cause and contributing factors

Describe the technical root cause, triggering condition, contributing design/process factors, and
the controls that should have prevented or limited impact. Include a causal chain rather than
stopping at the first human action or component failure.

## Recovery and verification

Record artifact and migration decisions, canary stages, SLO recovery, active-match and protocol
checks, hidden-state/winner/reward correctness, backup or deletion-ledger checks if applicable, and
the sustained monitoring period before resolution.

## What went well / where we were lucky / what needs improvement

Separate effective controls from chance. Do not treat the absence of observed harm as proof that a
control worked without evidence.

## Corrective actions

Every action must reduce a named recurrence, detection, mitigation, or blast-radius risk. One owner
and one due date are mandatory; “team” and “ongoing” are not valid ownership or deadlines.

| ID | Risk reduced | Action | Owner | Due date | Verification and status |
| --- | --- | --- | --- | --- | --- |
| `INC-YYYY-NNN-A1` | risk | measurable change | named role/person | `YYYY-MM-DD` | test, drill, metric, or review |

## Follow-up communication

Link the final public status update, affected-player/support guidance, security/privacy disclosure
decision where applicable, and the date on which action closure will be audited.
