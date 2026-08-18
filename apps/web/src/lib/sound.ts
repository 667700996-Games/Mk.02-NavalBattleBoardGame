import { get, type Unsubscriber } from 'svelte/store';
import { preferences, type AudioMix, type Preferences } from '$lib/stores';

type AudioBus = keyof AudioMix;
type RoutedBus = Exclude<AudioBus, 'master'>;
type AudioLifecycle = 'idle' | 'loading' | 'running' | 'suspended' | 'interrupted' | 'unavailable';

interface AudioAsset {
  url: string;
  bus: RoutedBus;
  loop?: boolean;
  gain?: number;
}

export const AUDIO_ASSETS = {
  musicCommand: { url: '/audio/music-command-loop.mp3', bus: 'music', loop: true, gain: 0.72 },
  ambienceOcean: {
    url: '/audio/ambience-ocean-loop.mp3',
    bus: 'ambience',
    loop: true,
    gain: 0.66
  },
  hover: { url: '/audio/ui-hover.mp3', bus: 'effects', gain: 0.5 },
  select: { url: '/audio/ui-select.mp3', bus: 'effects', gain: 0.72 },
  confirm: { url: '/audio/ui-confirm.mp3', bus: 'effects', gain: 0.78 },
  cancel: { url: '/audio/ui-cancel.mp3', bus: 'effects', gain: 0.7 },
  connected: { url: '/audio/ui-connected.mp3', bus: 'effects', gain: 0.68 },
  chat: { url: '/audio/ui-chat.mp3', bus: 'effects', gain: 0.58 },
  targetLock: { url: '/audio/ui-target-lock.mp3', bus: 'effects', gain: 0.72 },
  radar: { url: '/audio/ui-radar.mp3', bus: 'effects', gain: 0.54 },
  sonar: { url: '/audio/ui-sonar.mp3', bus: 'effects', gain: 0.7 },
  place: { url: '/audio/ui-place.mp3', bus: 'effects', gain: 0.72 },
  rotate: { url: '/audio/ui-rotate.mp3', bus: 'effects', gain: 0.65 },
  fire: { url: '/audio/weapon-fire.mp3', bus: 'effects', gain: 0.92 },
  miss: { url: '/audio/impact-miss.mp3', bus: 'effects', gain: 0.76 },
  hit: { url: '/audio/impact-hit.mp3', bus: 'effects', gain: 0.92 },
  sunk: { url: '/audio/vessel-sinking.mp3', bus: 'effects', gain: 0.96 },
  turnCue: { url: '/audio/cue-turn.mp3', bus: 'voice', gain: 0.72 },
  readyCue: { url: '/audio/cue-ready.mp3', bus: 'voice', gain: 0.72 },
  startCue: { url: '/audio/cue-start.mp3', bus: 'voice', gain: 0.78 },
  countdownCue: { url: '/audio/cue-countdown.mp3', bus: 'voice', gain: 0.68 },
  hitCue: { url: '/audio/cue-hit.mp3', bus: 'voice', gain: 0.72 },
  missCue: { url: '/audio/cue-miss.mp3', bus: 'voice', gain: 0.68 },
  sunkCue: { url: '/audio/cue-sunk.mp3', bus: 'voice', gain: 0.78 },
  victory: { url: '/audio/victory.mp3', bus: 'effects', gain: 0.9 },
  defeat: { url: '/audio/defeat.mp3', bus: 'effects', gain: 0.9 },
  victoryCue: { url: '/audio/cue-victory.mp3', bus: 'voice', gain: 0.78 },
  defeatCue: { url: '/audio/cue-defeat.mp3', bus: 'voice', gain: 0.78 }
} as const satisfies Record<string, AudioAsset>;

export type AudioAssetId = keyof typeof AUDIO_ASSETS;
export type HapticEvent =
  'select' | 'confirm' | 'fire' | 'miss' | 'hit' | 'sunk' | 'victory' | 'defeat';

export const HAPTIC_PATTERNS: Readonly<Record<HapticEvent, readonly number[]>> = {
  select: [8],
  confirm: [12, 28, 18],
  fire: [18, 24, 32],
  miss: [9],
  hit: [24, 18, 38],
  sunk: [30, 28, 55, 34, 80],
  victory: [18, 35, 18, 35, 55],
  defeat: [55, 36, 90]
};

const CRITICAL_AUDIO_ASSETS: readonly AudioAssetId[] = [
  'select',
  'confirm',
  'connected',
  'targetLock',
  'place',
  'rotate',
  'fire',
  'miss',
  'hit',
  'sunk',
  'turnCue',
  'readyCue',
  'startCue',
  'countdownCue',
  'hitCue',
  'missCue',
  'sunkCue',
  'victory',
  'defeat',
  'victoryCue',
  'defeatCue'
];

export function clampAudioLevel(value: number): number {
  return Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;
}

export function mixTargets(preference: Preferences): Record<AudioBus, number> {
  return {
    master: preference.sound ? clampAudioLevel(preference.audioMix.master) : 0,
    music: clampAudioLevel(preference.audioMix.music),
    effects: clampAudioLevel(preference.audioMix.effects),
    ambience: clampAudioLevel(preference.audioMix.ambience),
    voice: clampAudioLevel(preference.audioMix.voice)
  };
}

function audioContextConstructor(): typeof AudioContext | null {
  if (typeof globalThis === 'undefined') return null;
  const extended = globalThis as typeof globalThis & { webkitAudioContext?: typeof AudioContext };
  return extended.AudioContext ?? extended.webkitAudioContext ?? null;
}

class AudioDirector {
  private context: AudioContext | null = null;
  private master: GainNode | null = null;
  private buses: Partial<Record<RoutedBus, GainNode>> = {};
  private buffers = new Map<AudioAssetId, AudioBuffer>();
  private loading = new Map<AudioAssetId, Promise<AudioBuffer | null>>();
  private loops = new Map<AudioAssetId, AudioBufferSourceNode>();
  private unsubscribe: Unsubscriber | null = null;
  private cleanupListeners: (() => void) | null = null;
  private lifecycleSuspended = false;
  private lifecycle: AudioLifecycle = 'idle';
  private outputRevision = 0;
  private preloadScheduled = false;
  private preloadTimer: number | null = null;

  install(): () => void {
    if (typeof document === 'undefined' || this.cleanupListeners) return () => undefined;

    this.unsubscribe = preferences.subscribe((value) => this.applyMix(value));
    const unlock = () => {
      void this.unlock();
      document.removeEventListener('pointerdown', unlock, true);
      document.removeEventListener('keydown', unlock, true);
      document.removeEventListener('touchstart', unlock, true);
    };
    const onVisibility = () => {
      if (document.hidden) void this.suspendForLifecycle();
      else void this.resumeFromLifecycle();
    };
    const onPageHide = () => void this.suspendForLifecycle();
    const onPageShow = () => void this.resumeFromLifecycle();
    const onBlur = () => void this.suspendForLifecycle();
    const onFocus = () => void this.resumeFromLifecycle();
    const onDeviceChange = () => void this.refreshOutputDevice();

    document.addEventListener('pointerdown', unlock, true);
    document.addEventListener('keydown', unlock, true);
    document.addEventListener('touchstart', unlock, true);
    document.addEventListener('visibilitychange', onVisibility, true);
    window.addEventListener('pagehide', onPageHide, true);
    window.addEventListener('pageshow', onPageShow, true);
    window.addEventListener('blur', onBlur, true);
    window.addEventListener('focus', onFocus, true);
    navigator.mediaDevices?.addEventListener?.('devicechange', onDeviceChange);

    this.cleanupListeners = () => {
      document.removeEventListener('pointerdown', unlock, true);
      document.removeEventListener('keydown', unlock, true);
      document.removeEventListener('touchstart', unlock, true);
      document.removeEventListener('visibilitychange', onVisibility, true);
      window.removeEventListener('pagehide', onPageHide, true);
      window.removeEventListener('pageshow', onPageShow, true);
      window.removeEventListener('blur', onBlur, true);
      window.removeEventListener('focus', onFocus, true);
      navigator.mediaDevices?.removeEventListener?.('devicechange', onDeviceChange);
    };
    this.publishState();

    return () => this.dispose();
  }

  private publishState(): void {
    if (typeof document === 'undefined') return;
    document.documentElement.dataset.audioLifecycle = this.lifecycle;
    document.documentElement.dataset.audioOutputRevision = String(this.outputRevision);
    document.documentElement.dataset.audioLoadedAssets = String(this.buffers.size);
  }

  private createContext(): AudioContext | null {
    if (this.context) return this.context;
    const Constructor = audioContextConstructor();
    if (!Constructor) {
      this.lifecycle = 'unavailable';
      this.publishState();
      return null;
    }
    this.context = new Constructor({ latencyHint: 'interactive' });
    this.master = this.context.createGain();
    this.master.connect(this.context.destination);
    for (const bus of ['music', 'effects', 'ambience', 'voice'] as const) {
      const node = this.context.createGain();
      node.connect(this.master);
      this.buses[bus] = node;
    }
    this.context.onstatechange = () => {
      if (!this.context) return;
      this.lifecycle =
        this.context.state === 'running'
          ? 'running'
          : this.lifecycleSuspended
            ? 'suspended'
            : 'interrupted';
      this.publishState();
    };
    this.applyMix(get(preferences));
    return this.context;
  }

  private async unlock(): Promise<AudioContext | null> {
    if (!get(preferences).sound || this.lifecycleSuspended || document.hidden) return null;
    const context = this.createContext();
    if (!context) return null;
    if (context.state !== 'running') {
      try {
        await context.resume();
      } catch {
        this.lifecycle = 'interrupted';
        this.publishState();
        return null;
      }
    }
    this.lifecycle = 'running';
    this.publishState();
    void this.startLoops();
    this.scheduleCriticalPreload();
    return context;
  }

  private scheduleCriticalPreload(): void {
    if (this.preloadScheduled || typeof window === 'undefined') return;
    this.preloadScheduled = true;
    const warm = () => {
      this.preloadTimer = null;
      void (async () => {
        for (const id of CRITICAL_AUDIO_ASSETS) {
          if (this.lifecycleSuspended || !get(preferences).sound) return;
          await this.load(id);
        }
      })();
    };
    this.preloadTimer = window.setTimeout(warm, 700);
  }

  private applyMix(preference: Preferences): void {
    if (!this.context || !this.master) return;
    const targets = mixTargets(preference);
    const now = this.context.currentTime;
    this.master.gain.setTargetAtTime(targets.master, now, 0.025);
    for (const bus of ['music', 'effects', 'ambience', 'voice'] as const) {
      this.buses[bus]?.gain.setTargetAtTime(targets[bus], now, 0.025);
    }
    if (preference.sound && !this.lifecycleSuspended) void this.startLoops();
    else this.stopLoops();
  }

  private async load(id: AudioAssetId): Promise<AudioBuffer | null> {
    const cached = this.buffers.get(id);
    if (cached) return cached;
    const pending = this.loading.get(id);
    if (pending) return pending;
    const context = this.context;
    if (!context) return null;
    this.lifecycle = 'loading';
    this.publishState();
    const request = fetch(AUDIO_ASSETS[id].url, { credentials: 'same-origin' })
      .then((response) => {
        if (!response.ok) throw new Error(`audio ${response.status}`);
        return response.arrayBuffer();
      })
      .then((data) => context.decodeAudioData(data.slice(0)))
      .then((buffer) => {
        this.buffers.set(id, buffer);
        if (context.state === 'running') this.lifecycle = 'running';
        this.publishState();
        return buffer;
      })
      .catch(() => {
        this.lifecycle = context.state === 'running' ? 'running' : 'interrupted';
        this.publishState();
        return null;
      })
      .finally(() => this.loading.delete(id));
    this.loading.set(id, request);
    return request;
  }

  async play(id: AudioAssetId, playbackRate = 1): Promise<void> {
    const preference = get(preferences);
    if (!preference.sound || this.lifecycleSuspended || document.hidden) return;
    const context = await this.unlock();
    if (!context || context.state !== 'running') return;
    const buffer = await this.load(id);
    if (!buffer || this.lifecycleSuspended) return;
    const asset = AUDIO_ASSETS[id];
    const bus = this.buses[asset.bus];
    if (!bus) return;
    const source = context.createBufferSource();
    const trim = context.createGain();
    source.buffer = buffer;
    source.playbackRate.value = playbackRate;
    trim.gain.value = asset.gain ?? 1;
    source.connect(trim).connect(bus);
    source.start();
  }

  private async startLoops(): Promise<void> {
    if (!get(preferences).sound || this.lifecycleSuspended || document.hidden) return;
    const context = this.context;
    if (!context || context.state !== 'running') return;
    for (const id of ['musicCommand', 'ambienceOcean'] as const) {
      if (this.loops.has(id)) continue;
      const buffer = await this.load(id);
      if (!buffer || this.loops.has(id) || this.lifecycleSuspended || !get(preferences).sound)
        continue;
      const asset = AUDIO_ASSETS[id];
      const bus = this.buses[asset.bus];
      if (!bus) continue;
      const source = context.createBufferSource();
      const trim = context.createGain();
      source.buffer = buffer;
      source.loop = true;
      trim.gain.value = asset.gain ?? 1;
      source.connect(trim).connect(bus);
      source.onended = () => this.loops.delete(id);
      source.start();
      this.loops.set(id, source);
    }
  }

  private stopLoops(): void {
    for (const source of this.loops.values()) {
      try {
        source.stop();
      } catch {
        // A source that already ended is safe to discard.
      }
      source.disconnect();
    }
    this.loops.clear();
  }

  private async suspendForLifecycle(): Promise<void> {
    this.lifecycleSuspended = true;
    this.stopLoops();
    if (this.preloadTimer !== null) window.clearTimeout(this.preloadTimer);
    this.preloadTimer = null;
    this.preloadScheduled = false;
    if (this.context && this.context.state === 'running')
      await this.context.suspend().catch(() => undefined);
    this.lifecycle = 'suspended';
    this.publishState();
  }

  private async resumeFromLifecycle(): Promise<void> {
    this.lifecycleSuspended = false;
    if (!get(preferences).sound || !this.context) {
      this.lifecycle = this.context ? 'suspended' : 'idle';
      this.publishState();
      return;
    }
    try {
      await this.context.resume();
      this.lifecycle = this.context.state === 'running' ? 'running' : 'interrupted';
      this.publishState();
      if (this.context.state === 'running') await this.startLoops();
    } catch {
      this.lifecycle = 'interrupted';
      this.publishState();
    }
  }

  private async refreshOutputDevice(): Promise<void> {
    this.outputRevision += 1;
    const wasRunning = this.context?.state === 'running' && !this.lifecycleSuspended;
    this.stopLoops();
    if (wasRunning && this.context) {
      await this.context.suspend().catch(() => undefined);
      await this.context.resume().catch(() => undefined);
      await this.startLoops();
    }
    this.publishState();
  }

  private dispose(): void {
    this.cleanupListeners?.();
    this.cleanupListeners = null;
    this.unsubscribe?.();
    this.unsubscribe = null;
    this.stopLoops();
    void this.context?.close();
    this.context = null;
    this.master = null;
    this.buses = {};
    this.lifecycle = 'idle';
    this.publishState();
  }
}

const director = new AudioDirector();

function haptic(event: HapticEvent): void {
  const preference = get(preferences);
  if (
    !preference.haptics ||
    typeof navigator === 'undefined' ||
    typeof navigator.vibrate !== 'function' ||
    typeof matchMedia === 'undefined' ||
    !matchMedia('(pointer: coarse)').matches ||
    document.hidden
  ) {
    return;
  }
  navigator.vibrate([...HAPTIC_PATTERNS[event]]);
}

function accessibilityCue(id: AudioAssetId): void {
  if (get(preferences).audioCues) void director.play(id);
}

export function installAudioDirector(): () => void {
  return director.install();
}

export const sounds = {
  hover: () => void director.play('hover'),
  select: () => {
    void director.play('select');
    haptic('select');
  },
  targetLock: () => void director.play('targetLock'),
  connected: () => void director.play('connected'),
  chat: () => void director.play('chat'),
  confirm: () => {
    void director.play('confirm');
    haptic('confirm');
  },
  cancel: () => void director.play('cancel'),
  radar: () => void director.play('radar'),
  sonar: () => void director.play('sonar'),
  place: () => {
    void director.play('place');
    haptic('select');
  },
  rotate: () => void director.play('rotate'),
  fire: () => {
    void director.play('fire');
    haptic('fire');
  },
  miss: () => {
    void director.play('miss');
    accessibilityCue('missCue');
    haptic('miss');
  },
  hit: () => {
    void director.play('hit');
    accessibilityCue('hitCue');
    haptic('hit');
  },
  sunk: () => {
    void director.play('sunk');
    accessibilityCue('sunkCue');
    haptic('sunk');
  },
  turn: () => {
    void director.play('connected', 1.08);
    accessibilityCue('turnCue');
  },
  ready: () => {
    void director.play('confirm');
    accessibilityCue('readyCue');
    haptic('confirm');
  },
  start: () => {
    void director.play('confirm', 0.92);
    accessibilityCue('startCue');
    haptic('confirm');
  },
  countdown: (seconds: number) => {
    void director.play('select', seconds <= 3 ? 1.18 : seconds <= 5 ? 1.08 : 0.96);
    accessibilityCue('countdownCue');
  },
  victory: () => {
    void director.play('victory');
    accessibilityCue('victoryCue');
    haptic('victory');
  },
  defeat: () => {
    void director.play('defeat');
    accessibilityCue('defeatCue');
    haptic('defeat');
  },
  preview: (bus: RoutedBus) => {
    const preview: Record<RoutedBus, AudioAssetId> = {
      music: 'musicCommand',
      effects: 'confirm',
      ambience: 'ambienceOcean',
      voice: 'turnCue'
    };
    void director.play(preview[bus]);
  }
};
