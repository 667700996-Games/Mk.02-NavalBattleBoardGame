# MK.01 production art bible

Owner: Web Experience · Art Direction  
Approval state: launch presentation baseline  
Last reviewed: 2026-08-19

## Visual thesis

MK.01 is a naval command room, not a toy tabletop. The player should read the board in less than a
second and then notice the material detail: near-black steel, cold water, fine sonar light and short,
decisive ordnance flashes. Atmosphere may frame tactical information but must never compete with it.

The hierarchy is fixed: current turn and selected coordinate, legal action, hit/miss/sunk state,
fleet condition, then decoration. A cosmetic or effect that changes this order is rejected.

## Shape language

- Command surfaces use clipped or asymmetric corners, one-pixel technical edges and sparse bracket
  marks. Primary actions remain broad rectangles with a clear silhouette at 320 px.
- Friendly vessels use long, low hull silhouettes. Carrier, battleship, cruiser, submarine and
  destroyer have distinct deck/island/turret profiles in `Vessel.svelte`; the silhouette is invariant
  across skins.
- Targets are circular and concentric. A miss is a ring plus wave glyph; a hit is a filled core plus
  flame glyph; a sunk cell retains its hull silhouette and a critical-state treatment.
- Profile emblems only alter the frame around initials: circular anchor, shield-like trident and
  rotated compass. Initials, presence dot and accessible name remain unchanged.

## Color and material

Core palette: abyss `#03111d`, command navy `#06283b`, deep cyan `#0b5970`, signal cyan `#62d9e8`,
safe green, warning amber and critical red. Water highlights stay sparse so white labels and cyan grid
lines preserve contrast. Critical states always include copy, a glyph or a shape in addition to color.

Fleet skins change only hull/deck/island material. Board themes change only water tint and surface
energy. Effect themes change only the hit accent. The semantic tokens (`--tactical`, `--safe`,
`--warning`, `--critical`) and color-vision presets remain authoritative.

## Motion grammar

- Selection: 120–180 ms, one local response. Never move the selected coordinate.
- Weapon sequence: acquire, fire, impact. The persistent board marker is visible after the transient
  sequence ends.
- Sinking and result transitions may use scale, opacity and light, but must not obscure the result
  copy or controls.
- `prefers-reduced-motion` and the in-game reduced-motion preference reduce every animation to a
  single near-instant frame. State changes remain announced through the live region.

## Typography

Rajdhani is the display face for coordinates, telemetry and short headings. IBM Plex Sans KR is the
body and Korean UI face. Uppercase Latin labels use at least `0.08em` tracking; Korean copy is never
artificially spaced. Body text targets 14 px or larger, compact telemetry 10 px or larger, and
interactive labels never rely on condensed text alone.

## Readability invariants

1. Fog-of-war data is determined exclusively by the authoritative snapshot; cosmetic preferences are
   local strings and never enter an API or WebSocket command.
2. Every attack outcome has persistent text/ARIA semantics and a distinct glyph/shape.
3. Board coordinates, focus rings and enabled targets remain readable in every color-vision mode,
   theme and effects tier.
4. High contrast overrides presentation colors. Reduced motion overrides decorative motion.
5. The 320 px minimum viewport has no horizontal document overflow and keeps the active control in
   reach.

## Asset and effects tiers

| Tier | Intended hardware | Water | Lighting/effects | Tactical markers |
| --- | --- | --- | --- | --- |
| High | default desktop/mobile | approved ocean texture plus tint | layered glow, short pulses | full, persistent |
| Low | battery/low GPU | approved texture plus tint | reduced blur/shadow, one pulse | full, persistent |
| Minimal | constrained/recovery | two CSS gradients; no texture fetch | no blur, shadow or animation | full, persistent |

Effects tiers are user selectable and persisted locally. They do not reduce board resolution, hide
ships, remove markers or alter timing.

## Launch asset manifest and provenance

| Asset | Source | Role | Approval notes |
| --- | --- | --- | --- |
| `static/art/ocean-command-surface-v1.webp` | Generated specifically for MK.01 with OpenAI image generation on 2026-08-19; no reference image or third-party asset supplied | high/low board water | 768 px WebP, dark orthographic water, no text/ships/land; safe beneath a 10×10 grid |
| `Vessel.svelte` | original project SVG geometry | five ship silhouettes and three finishes | vector at every viewport; sunk/invalid states override cosmetics |
| `GridBoard.svelte` | original project CSS/SVG composition | targeting, water, hit, miss, sunk | semantic glyphs and minimal tier included |
| `BattleView.svelte` | original project UI/VFX | acquire/fire/impact and battle transitions | reduced-motion behavior included |
| `ResultView.svelte` | original project UI/SVG composition | victory, defeat, declassified board | outcome copy and controls remain above decoration |

The ocean generation brief requested a square, orthographic deep-navy command surface with subtle
cyan caustics, restrained waves, no central focal point, no grid, ships, land, text, logo or watermark.
The generated source was visually reviewed before conversion to a 768 px WebP. No downloaded stock
media is present in the launch art manifest.

## Approval and regression

The approved reference set contains landing, lobby, practice combat with a persistent hit, and the
after-action defeat report at desktop and mobile viewports. `npm run test:visual` uses deterministic
time, locale, fonts, reduced motion and a one-percent maximum changed-pixel ratio. Any intentional art
change requires replacing the affected reference capture, review by Web Experience, and an update to
this manifest when asset provenance or tier behavior changes.
