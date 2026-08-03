import { browser } from '$app/environment';
import { writable } from 'svelte/store';
import type { AttackRecord, GameSnapshot, ProtocolError, Session } from '$lib/types';

export type SocketStatus = 'idle' | 'connecting' | 'online' | 'reconnecting' | 'offline';

export const session = writable<Session | null>(null);
export const gameSnapshot = writable<GameSnapshot | null>(null);
export const socketStatus = writable<SocketStatus>('idle');
export const lastAttack = writable<AttackRecord | null>(null);
export const gameError = writable<ProtocolError | null>(null);

export interface Preferences {
  sound: boolean;
  reducedMotion: boolean;
  highContrast: boolean;
}

const defaults: Preferences = {
  sound: true,
  reducedMotion: false,
  highContrast: false
};

function loadPreferences(): Preferences {
  if (!browser) return defaults;
  try {
    return { ...defaults, ...JSON.parse(localStorage.getItem('mk01_preferences') ?? '{}') };
  } catch {
    return defaults;
  }
}

export const preferences = writable<Preferences>(loadPreferences());

if (browser) {
  preferences.subscribe((value) => {
    localStorage.setItem('mk01_preferences', JSON.stringify(value));
    document.documentElement.dataset.motion = value.reducedMotion ? 'reduced' : 'full';
    document.documentElement.dataset.contrast = value.highContrast ? 'high' : 'normal';
  });
}
