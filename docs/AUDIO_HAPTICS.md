# Production audio and haptics

Owner: Web Experience · Audio  
Approval state: launch mix  
Last reviewed: 2026-08-19

## Direction

MK.01 uses a restrained naval command-room soundscape. Low drones and filtered ocean energy provide
space without masking tactical information. UI sounds are short and dry; weapons carry a low impact;
hit, miss and sinking remain distinguishable even at low volume. Victory resolves as a clear major
cluster and defeat descends without an exaggerated cinematic sting.

All 28 launch masters are file-backed MP3 assets. Runtime oscillator synthesis is forbidden by
`npm run audio:check`. The source masters were procedurally composed and rendered offline for this
project; no stock recording, sample pack or third-party copyrighted audio was ingested.

## Asset inventory

| Group | Masters | Route |
| --- | ---: | --- |
| Music | command-room loop | music |
| Ambience | filtered ocean/hull loop | ambience |
| UI | hover, select, confirm, cancel, connection, chat, lock, radar, sonar, place, rotate | effects |
| Combat | weapon fire, miss impact, hit impact, vessel sinking | effects |
| Outcome | victory, defeat | effects |
| Accessibility | turn, ready, start, countdown, hit, miss, sunk, victory, defeat earcons | voice/cues |

The complete payload is 424,592 bytes. Individual files are capped at 800 KB and the aggregate at
4 MB. `config/audio-assets.json` pins every byte count and SHA-256. `npm run audio:generate` rebuilds
the original 48 kHz procedural masters with ffmpeg; `npm run audio:check` verifies hashes, roles,
payload limits and the required lifecycle implementation.

## Mixer and playback

The Web Audio director exposes master, music, effects, ambience and voice/cue gain stages. Values are
independently adjustable from 0–100%, persisted per device and smoothed over 25 ms to avoid zipper
noise. The global sound switch changes only the master output and stops loops; it does not destroy the
saved channel balance.

Audio is lazily decoded. No asset is fetched during an untouched page load. The first keyboard,
pointer or touch gesture unlocks the context and begins the music and ambience loops, satisfying
browser autoplay policy. Critical combat and outcome masters warm sequentially after a short idle
delay so first fire and impact frames do not pay decode cost; every decoded buffer is then cached.
All routing terminates at the same master node, so muting and output interruption cannot leave an
orphaned sound.

## Lifecycle and devices

- `visibilitychange`, window blur and `pagehide` stop loops and suspend the context.
- Visibility, focus and `pageshow` resume the context only when sound remains enabled, then rebuild
  the loops.
- Browser-level interruptions are observed through `AudioContext.onstatechange` and represented as
  `interrupted` until a legal resume succeeds.
- `devicechange` rebuilds active loop routes after a suspend/resume cycle so the browser destination
  follows the current system output.
- Root-layout teardown removes every listener, stops sources, disconnects loops and closes the
  context.

Diagnostic HTML data attributes expose only lifecycle state, decoded-asset count and a monotonically
increasing output revision. They contain no device name, hardware identifier or user information.

## Accessibility cues

The voice/cue bus contains redundant earcons for turn, ready/start, countdown, hit, miss, sunk,
victory and defeat. They supplement persistent text, glyphs and live-region announcements; they never
replace them. Players may disable these cues independently or lower their bus without muting combat
effects. Music and ambience are never placed on the cue bus.

## Haptic grammar

Haptics run only when all three conditions are true: the player enabled them, the browser implements
`navigator.vibrate`, and the active input reports a coarse pointer. Unsupported devices silently keep
the visual and audio response.

| Event | Pattern (ms) | Intent |
| --- | --- | --- |
| Select | `8` | light confirmation |
| Confirm | `12–28–18` | accepted action |
| Fire | `18–24–32` | launch and recoil |
| Miss | `9` | intentionally light water contact |
| Hit | `24–18–38` | solid contact |
| Sunk | `30–28–55–34–80` | escalating hull loss |
| Victory | `18–35–18–35–55` | rising confirmation cadence |
| Defeat | `55–36–90` | two heavy pulses |

Every pattern ends with a vibration segment, contains no pulse over 100 ms and stays within 300 ms
total. There is no hover vibration, continuous motor use or vibration when the page is hidden.

## Verification

`sound.test.ts` proves manifest-to-runtime coverage, mixer clamping/muting and bounded haptic patterns.
`audio-haptics.spec.ts` proves music, ambience and UI masters decode after a user gesture in Chromium,
Firefox and WebKit. Its Chromium lifecycle fixture verifies all five persisted sliders, the global
switch, independent accessibility cues, optional coarse-pointer vibration, background suspend/focus
resume, output-device revision and actual asset requests. The audio asset verifier is part of the
root lint/release chain.
