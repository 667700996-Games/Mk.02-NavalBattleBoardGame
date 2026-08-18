## Player and service impact

- Player-visible behavior:
- Authoritative state or hidden-information impact:
- Persistence, privacy, protocol, security, SLO, or rollback impact:

## Boundaries and owners

- Architecture boundary IDs changed:
- Accountable owner role(s):
- Required reviewer role(s):
- Cross-boundary approval evidence:

## Decision and compatibility

- ADR added/updated, or reason no ADR is required:
- Stable/candidate client-server compatibility evidence:
- Migration expand/migrate/contract evidence:
- Active-match, replay, rollback, and data-deletion effect:

## Verification

- [ ] `npm run architecture:check`
- [ ] `npm run check`
- [ ] `npm run lint`
- [ ] `npm test`
- [ ] Relevant PostgreSQL/Redis, browser, accessibility, performance, or recovery gate
- [ ] CODEOWNERS review requirements are satisfied

Independent approval is mandatory for authentication/authorization, hidden-state visibility,
migration/retention, protocol windows, and production security/SLO changes. See
`docs/ARCHITECTURE.md` and `.github/architecture-ownership.json`.
