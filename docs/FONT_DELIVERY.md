# Font Delivery Contract

Mk.01 ships Korean interface copy as deterministic WOFF2 subsets. The release artifact must not
contain the monolithic IBM Plex Sans KR Korean files or legacy WOFF files.

## Generation and coverage

`npm run fonts:generate` scans every Rust, Svelte, and TypeScript production source file for
Hangul/Jamo code points. It maps those code points to Fontsource's disjoint IBM Plex Sans KR slices
for weights 400 and 700, orders the fallback families by observed source-glyph coverage, and writes
`apps/web/src/fonts.css`. Latin IBM Plex and the Rajdhani display faces remain separate WOFF2
assets. Arbitrary player nicknames, room names, and chat characters that are not in the static
product copy fall through to the platform Korean font; they do not force the full Korean webfont to
download.

`npm run fonts:check` regenerates the contract in memory and fails when the committed CSS is stale,
a production Hangul glyph is uncovered, an asset is selected twice, or the two Korean weights
exceed 450,000 bytes. It runs inside the root lint gate, so adding copy requires regenerating and
reviewing the selected slices. The generated faces reference only `.woff2`; the bundle gate rejects
any legacy `.woff` artifact.

## August 2026 baseline

| Measurement | Previous | Current | Gate |
| --- | ---: | ---: | ---: |
| Korean 400/700 payload | 1,014,868 B | 406,344 B | ≤ 450,000 B |
| Complete WOFF2 artifact | 1,091,828 B | 483,304 B | ≤ 1,200,000 B |
| Selected Korean slices | 2 monoliths | 28 disjoint slices | no duplicates |
| Font format | WOFF2 | WOFF2 only | legacy WOFF rejected |

The complete font artifact is 55.7% smaller without raising the existing JS, CSS, per-file, or font
budgets. `font-delivery.spec.ts` observes real Chromium font responses, rejects monolithic or legacy
assets and duplicate requests, loads both Korean weights, and caps the route transfer at 500,000
bytes. The normal full-match and responsive browser matrix remains the visual/layout regression
gate for the changed fallback stack.
