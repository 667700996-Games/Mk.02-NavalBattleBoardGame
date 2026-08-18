import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { AUDIO_ASSETS, HAPTIC_PATTERNS, clampAudioLevel, mixTargets } from './sound';
import type { Preferences } from './stores';

const preference: Preferences = {
  sound: true,
  audioMix: { master: 0.8, music: 0.55, effects: 0.85, ambience: 0.5, voice: 0.8 },
  audioCues: true,
  haptics: true,
  reducedMotion: false,
  effectQuality: 'high',
  highContrast: false,
  colorVision: 'standard',
  cosmetics: {
    fleetSkin: 'steel',
    boardTheme: 'abyss',
    effectTheme: 'signal',
    profileEmblem: 'anchor',
    presentationFrame: 'command'
  },
  tutorialCompleted: false
};

describe('production audio contract', () => {
  it('routes every runtime sound to a hashed launch asset', () => {
    const manifest = JSON.parse(
      readFileSync(new URL('../../../../config/audio-assets.json', import.meta.url), 'utf8')
    ) as { assets: { path: string; bus: string }[] };
    const manifested = new Set(
      manifest.assets.map((asset) => `/${asset.path.split('/static/')[1]}`)
    );
    const runtime = Object.values(AUDIO_ASSETS);

    expect(runtime).toHaveLength(28);
    expect(new Set(runtime.map((asset) => asset.url)).size).toBe(runtime.length);
    expect(runtime.every((asset) => manifested.has(asset.url))).toBe(true);
    expect(new Set(runtime.map((asset) => asset.bus))).toEqual(
      new Set(['music', 'effects', 'ambience', 'voice'])
    );
  });

  it('clamps malformed mixer input and silences only the master when globally disabled', () => {
    expect(clampAudioLevel(-3)).toBe(0);
    expect(clampAudioLevel(2)).toBe(1);
    expect(clampAudioLevel(Number.NaN)).toBe(0);
    expect(mixTargets(preference)).toEqual(preference.audioMix);
    expect(mixTargets({ ...preference, sound: false })).toEqual({
      ...preference.audioMix,
      master: 0
    });
  });

  it('keeps haptic patterns bounded, intentional, and pulse-terminated', () => {
    for (const pattern of Object.values(HAPTIC_PATTERNS)) {
      expect(pattern.length % 2).toBe(1);
      expect(pattern.every((duration) => duration > 0 && duration <= 100)).toBe(true);
      expect(pattern.reduce((total, duration) => total + duration, 0)).toBeLessThanOrEqual(300);
    }
  });
});
