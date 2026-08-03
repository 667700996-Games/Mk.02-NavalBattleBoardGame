import { get } from 'svelte/store';
import { preferences } from '$lib/stores';

let audioContext: AudioContext | null = null;

function tone(frequency: number, duration: number, gain = 0.035, slideTo?: number): void {
  if (!get(preferences).sound || typeof AudioContext === 'undefined') return;
  audioContext ??= new AudioContext();
  const oscillator = audioContext.createOscillator();
  const volume = audioContext.createGain();
  const now = audioContext.currentTime;
  oscillator.type = 'sine';
  oscillator.frequency.setValueAtTime(frequency, now);
  if (slideTo) oscillator.frequency.exponentialRampToValueAtTime(slideTo, now + duration);
  volume.gain.setValueAtTime(gain, now);
  volume.gain.exponentialRampToValueAtTime(0.0001, now + duration);
  oscillator.connect(volume).connect(audioContext.destination);
  oscillator.start(now);
  oscillator.stop(now + duration);
}

export const sounds = {
  select: () => tone(420, 0.08, 0.02, 520),
  miss: () => tone(170, 0.22, 0.025, 90),
  hit: () => tone(110, 0.28, 0.06, 45),
  sunk: () => {
    tone(150, 0.38, 0.06, 42);
    setTimeout(() => tone(80, 0.44, 0.04, 38), 120);
  },
  turn: () => tone(560, 0.12, 0.025, 760),
  victory: () => {
    [440, 554, 659].forEach((frequency, index) =>
      setTimeout(() => tone(frequency, 0.35, 0.035), index * 130)
    );
  }
};

