# Performance budget contract

`config/performance-budgets.json` is the single source of truth for Mk.01 client artifact and
production-runtime performance gates. A budget change requires a measured before/after result and
review; a failing candidate must not raise a limit merely to make CI green.

## Artifact gate

`npm run build && npm run budget` walks the complete SvelteKit client artifact. It enforces per-file
and total raw-byte limits for JavaScript, CSS, WOFF2 fonts, images, and audio, and rejects legacy
WOFF files. The current production artifact is:

| Category | Measured bytes | Total budget |
| --- | ---: | ---: |
| JavaScript | 317,857 | 320,000 |
| CSS | 184,601 | 185,000 |
| WOFF2 fonts | 483,304 | 1,200,000 |
| Images | 2,137,055 | 2,200,000 |
| Audio | 0 | 4,000,000 |

The image total includes both static Open Graph images; route transfer is measured separately and
does not fetch them. Font generation and the tighter Korean subset limit are documented in
`FONT_DELIVERY.md`.

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
| Desktop | 256,584 / 144,713 / 429,840 B | 12.1 MiB | 1,238 ms | 0 ms | 16.7 / 33.7 ms | 54,588 B |
| Mobile | 256,584 / 144,713 / 429,840 B | 9.3 MiB | 2,285 ms | 0 ms | 16.7 / 18.4 ms | 54,608 B |
| Low mobile | 256,584 / 144,713 / 429,840 B | 12.1 MiB | 6,106 ms | 0 ms | 16.7 / 18.6 ms | 54,645 B |

Large translucent surfaces formerly used nested `backdrop-filter` blurs. The reference desktop
sequence measured 66.7 ms frame p95 before those redundant filters were removed and 33.7 ms in the
five-hit reference run, while the existing opaque gradients, borders, and shadows preserved the
visual hierarchy.

## Interpretation and release use

- Artifact totals prevent unvisited routes and social images from silently growing the release.
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
