import { browser } from '$app/environment';
import { writable } from 'svelte/store';
import type {
  AttackRecord,
  ChatMessage,
  ChatTypingEvent,
  GameSnapshot,
  ProtocolError,
  Session
} from '$lib/types';

export type SocketStatus = 'idle' | 'connecting' | 'online' | 'reconnecting' | 'offline';
export type InputModality = 'pointer' | 'keyboard' | 'touch';

export const session = writable<Session | null>(null);
export const gameSnapshot = writable<GameSnapshot | null>(null);
export const socketStatus = writable<SocketStatus>('idle');
export const inputModality = writable<InputModality>('pointer');
export const lastAttack = writable<AttackRecord | null>(null);
export const gameError = writable<ProtocolError | null>(null);
export const chatMessages = writable<ChatMessage[]>([]);
export const chatTyping = writable<ChatTypingEvent | null>(null);
export const chatHistoryLoaded = writable(false);

export interface HudNotification {
  id: string;
  title: string;
  message: string;
  tone: 'info' | 'success' | 'warning' | 'danger';
}

export const hudNotifications = writable<HudNotification[]>([]);

export function dismissHudNotification(id: string): void {
  hudNotifications.update((notifications) =>
    notifications.filter((notification) => notification.id !== id)
  );
}

export function resetRoomRealtimeState(): void {
  chatMessages.set([]);
  chatTyping.set(null);
  chatHistoryLoaded.set(false);
}

export type ColorVisionMode = 'standard' | 'protanopia' | 'deuteranopia' | 'tritanopia';
export type EffectQuality = 'high' | 'low' | 'minimal';
export type FleetSkin = 'steel' | 'arctic' | 'ember';
export type BoardTheme = 'abyss' | 'sonar' | 'ice';
export type EffectTheme = 'signal' | 'plasma' | 'ordnance';
export type ProfileEmblem = 'anchor' | 'trident' | 'compass';
export type PresentationFrame = 'command' | 'stealth' | 'veteran';

export interface AudioMix {
  master: number;
  music: number;
  effects: number;
  ambience: number;
  voice: number;
}

export interface CosmeticLoadout {
  fleetSkin: FleetSkin;
  boardTheme: BoardTheme;
  effectTheme: EffectTheme;
  profileEmblem: ProfileEmblem;
  presentationFrame: PresentationFrame;
}

export interface Preferences {
  sound: boolean;
  audioMix: AudioMix;
  audioCues: boolean;
  haptics: boolean;
  reducedMotion: boolean;
  effectQuality: EffectQuality;
  highContrast: boolean;
  colorVision: ColorVisionMode;
  cosmetics: CosmeticLoadout;
  tutorialCompleted: boolean;
}

const defaults: Preferences = {
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

function loadPreferences(): Preferences {
  if (!browser) return defaults;
  try {
    const stored = JSON.parse(
      localStorage.getItem('mk01_preferences') ?? '{}'
    ) as Partial<Preferences>;
    return {
      ...defaults,
      ...stored,
      audioMix: { ...defaults.audioMix, ...stored.audioMix },
      cosmetics: { ...defaults.cosmetics, ...stored.cosmetics }
    };
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
    document.documentElement.dataset.colorVision = value.colorVision;
    document.documentElement.dataset.effectQuality = value.effectQuality;
    document.documentElement.dataset.fleetSkin = value.cosmetics.fleetSkin;
    document.documentElement.dataset.boardTheme = value.cosmetics.boardTheme;
    document.documentElement.dataset.effectTheme = value.cosmetics.effectTheme;
    document.documentElement.dataset.profileEmblem = value.cosmetics.profileEmblem;
    document.documentElement.dataset.presentationFrame = value.cosmetics.presentationFrame;
  });
}
