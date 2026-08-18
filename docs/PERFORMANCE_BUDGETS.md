# Performance budget contract

`config/performance-budgets.json` is the single source of truth for Mk.01 client artifact and
production-runtime performance gates. A budget change requires a measured before/after result and
review; a failing candidate must not raise a limit merely to make CI green.

## Artifact gate

`npm run build && npm run budget` walks the complete SvelteKit client artifact. It enforces per-file
and total raw-byte limits for JavaScript, CSS, WOFF2 fonts, images, and audio, rejects legacy WOFF
files, and resolves configured routes through SvelteKit's generated route dictionary to cap their
code-split entry JavaScript and CSS. The current production artifact is:

| Category | Measured bytes | Total budget |
| --- | ---: | ---: |
| JavaScript | 340,709 | 341,000 |
| CSS | 197,747 | 198,000 |
| WOFF2 fonts | 505,756 | 1,200,000 |
| Images | 2,137,055 | 2,200,000 |
| Audio | 0 | 4,000,000 |

The image total includes both static Open Graph images; route transfer is measured separately and
does not fetch them. Font generation and the tighter Korean subset limit are documented in
`FONT_DELIVERY.md`.

The post-match analysis increased the complete artifact from 317,857 to 329,778 JavaScript bytes,
184,601 to 190,787 CSS bytes, and 483,304 to 505,756 WOFF2 bytes. The feature is isolated in the
unvisited replay route, whose entry measures 18,187 JavaScript and 9,779 CSS bytes and is capped at
20,000 and 10,500 bytes respectively. The complete-artifact ceilings were raised only to admit this
measured code-split feature; the critical gameplay runtime limits remain 320 KB JavaScript and
185 KB CSS, so an increase on the landing-to-result journey still fails every device tier.

The versioned season/event presentation added 2,512 JavaScript and 1,907 CSS bytes to the complete
artifact without entering the gameplay journey.

The authoritative leaderboard expanded the last verified artifact from 334,978 to 340,709
JavaScript bytes and from 193,720 to 197,747 CSS bytes after reusing the route's existing icon
assets to remove another 1,199 JavaScript bytes. The complete-artifact ceilings moved only by the
measured 5,731 JavaScript and 4,027 CSS byte deltas, rounded to 341,000 and 198,000 bytes. `/stats`
remains an isolated route entry, now measured at 18,350 JavaScript and 13,473 CSS bytes and capped at
19,000 and 14,000 bytes. The landing-to-result runtime ceilings remain 320 KB JavaScript and 185 KB
CSS; the complete production journey is still measured independently on every device tier.

## Runtime gate

`npm run test:performance` builds and serves the production adapter, then completes a real practice
journey from landing through session creation and placement. It targets the deterministic practice
carrier at A1–A5, verifies four hits and the final sinking sequence, then exercises surrender
confirmation and the result. Chromium DevTools Protocol applies the configured CPU throttle and
reports task time and peak sampled heap use. Playwright records decoded unique response bodies,
long tasks, animation-frame intervals, and WebSocket frames.

| Tier | Viewport | CPU throttle | Frame p95 budget | CPU task budget | Long-task budget |
| --- | --- | ---: | ---: | ---: | ---: |
| Desktop | 1440×900 | 1× | 34 ms | 4,000 ms | 500 ms |
| Mobile | 412×915 | 3× | 67 ms | 8,000 ms | 1,000 ms |
| Low mobile | 360×640 | 6× | 100 ms | 12,000 ms | 2,000 ms |

All tiers also cap route JavaScript at 320 KB, CSS at 185 KB, fonts at 500 KB, images at 1.2 MB,
audio at 500 KB, JavaScript heap at 64 MiB, and WebSocket traffic at 75 KB for the journey.

The August 18, 2026 reference run passed all tiers:

| Tier | JS / CSS / fonts | Heap | CPU tasks | Long tasks | Frame p50 / p95 | WebSocket |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Desktop | 259,738 / 145,971 / 429,840 B | 12.0 MiB | 1,281 ms | 0 ms | 16.7 / 33.4 ms | 55,171 B |
| Mobile | 259,738 / 145,971 / 429,840 B | 14.1 MiB | 3,253 ms | 0 ms | 16.7 / 17.6 ms | 59,562 B |
| Low mobile | 259,738 / 145,971 / 429,840 B | 11.4 MiB | 7,271 ms | 0 ms | 16.7 / 17.6 ms | 59,317 B |

Large translucent surfaces formerly used nested `backdrop-filter` blurs. The reference desktop
sequence measured 66.7 ms frame p95 before those redundant filters were removed and 33.7 ms in the
five-hit reference run, while the existing opaque gradients, borders, and shadows preserved the
visual hierarchy.

Resolved hit and miss markers also used to repaint their glow or water ring indefinitely. Those
effects now run twice and settle into the same readable final marker; reduced-motion users receive
the settled state immediately. Three consecutive desktop reference runs measured 33.4, 33.4, and
33.5 ms frame p95 before the full three-tier gate above passed without changing any runtime limit.

## Interpretation and release use

- Artifact totals prevent unvisited routes and social images from silently growing the release.
- Route-entry totals prevent the larger post-match analysis from consuming its new headroom without
  an explicit measured budget review.
- Runtime transfer totals cover only resources actually loaded by the critical gameplay journey.
- Task duration is cumulative renderer CPU time; long-task duration captures main-thread stalls of
  at least 50 ms; frame p95 covers repeated target lock, hit/sinking feedback, modal, and result
  transition.
- Every tier rejects horizontal overflow and controls smaller than the documented readable/touch
  floor. The low-mobile report attaches 360×640 carrier-sunk and unobscured result captures.
- Synthetic CI is paired with aggregate field histograms for LCP, CLS, INP, and attack-command to
  authoritative-result latency. The field collector uses only bounded route/device labels, keeps no
  player or request identifier, and flushes queued observer entries when the page hides.
- CI attaches one JSON report per tier. Compare candidate and stable runs on the same runner class;
  investigate material movement even when both remain below the hard limit.

The field dashboard queries, minimum sample counts, good/poor thresholds, and canary stop rules are
defined in `OPERATIONS.md`. `performance-rum.spec.ts` proves a real Chromium lifecycle and practice
attack reach every histogram; the server integration test proves arbitrary dimensions, identifiers,
and out-of-range samples are rejected.
