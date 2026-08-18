# Localization policy

Mk.01 ships one complete copy contract across two launch locales:

| Locale  | Role                               | Source                                      |
| ------- | ---------------------------------- | ------------------------------------------- |
| `ko-KR` | Default and fallback launch locale | `apps/web/src/lib/i18n/messages/ko-KR.json` |
| `en-US` | English launch locale              | `apps/web/src/lib/i18n/messages/en-US.json` |
| `en-XA` | Test-only expanded pseudo locale   | Generated at runtime from `en-US`           |

`config/localization-policy.json` is the machine-readable authority. A chosen locale persists in
local storage and a same-site cookie, drives the document language and locale marker, and otherwise
falls back from a supported persisted value to the browser language and then Korean. The selector
exposes launch locales only; `en-XA` appears only when a test explicitly activates it.

## Copy contract

User-facing copy belongs in both launch catalogs under the same stable key. Placeholders use
`{name}` syntax and must be identical across catalogs. Catalog values contain text only, never HTML.
Components consume the typed `t` store; non-component presentation logic receives a translator or
uses the current-locale `message` helper. Dates, numbers, and relative time use the shared Intl
formatters rather than fixed separators or locale literals.

Each launch catalog is emitted as a dedicated `locale-<locale>` browser chunk. Korean is available
for the initial fallback render, while English loads only when selected or restored. The artifact
budget checks both the exact locale set and per-file/aggregate transfer ceilings.

Structured API and realtime failures are translated from their stable error code. The server's
human-readable message is diagnostic input and is never surfaced as launch-locale copy. Unknown or
non-API failures use a localized, context-specific fallback. Operator screens may append the stable
error code for support correlation without exposing the server-language message.

Brand marks, coordinates, checksums, protocol identifiers, and short military instrumentation tokens
are data or visual notation rather than prose. The static source gate maintains an explicit narrow
allowlist for those values; new prose must not be added to it.

## Automated acceptance

- `npm run localization:check` validates policy, full key and placeholder parity, non-empty plain
  text, source extraction, Intl use, pseudolocalization policy, and fallback font configuration.
- `npm --workspace @mk01/web run test:unit -- src/lib/i18n/i18n.test.ts` proves every key expands by
  at least 25%, placeholders survive, catalogs remain structurally identical, Intl output follows
  the active locale, and server error codes translate independently of server messages.
- `npm run test:localization` verifies English and `en-XA` persistence, document metadata, measured
  text expansion, locale switching, and absence of horizontal overflow on all six supported browser
  and device profiles. The test is also part of the full Playwright CI job.
- `npm run fonts:check` includes localized JSON in glyph discovery, rejects uncovered glyphs or stale
  generated CSS, and uses the strictest device font transfer budget from
  `config/performance-budgets.json`.
- `npm run budget` requires one bounded browser chunk for every launch locale and rejects missing,
  unexpected, or oversized locale payloads alongside the existing route and artifact budgets.

The root lint chain runs the static localization and font gates. A copy change is not complete until
both catalogs, focused unit tests, generated font CSS when needed, and the browser expansion gate
pass.
