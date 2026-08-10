import { get } from 'svelte/store';
import { preferences } from '$lib/stores';

let audioContext: AudioContext | null = null;

function tone(frequency: number, duration: number, gain = 0.035, slideTo?: number): void {
  if (!get(preferences).sound || typeof AudioContext === 'undefined') return;
  audioContext ??= new AudioContext();
  if (audioContext.state === 'suspended') void audioContext.resume();
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
  hover: () => tone(290, 0.045, 0.012, 350),
  select: () => tone(420, 0.08, 0.02, 520),
  targetLock: () => {
    tone(620, 0.08, 0.018, 840);
    setTimeout(() => tone(980, 0.055, 0.012, 760), 65);
  },
  connected: () => tone(390, 0.12, 0.016, 510),
  chat: () => tone(300, 0.075, 0.014, 430),
  confirm: () => {
    tone(520, 0.1, 0.025, 680);
    setTimeout(() => tone(760, 0.12, 0.018, 820), 70);
  },
  cancel: () => tone(220, 0.1, 0.018, 150),
  radar: () => tone(250, 0.18, 0.012, 390),
  sonar: () => tone(135, 0.3, 0.022, 70),
  place: () => tone(330, 0.12, 0.026, 460),
  rotate: () => tone(480, 0.1, 0.02, 300),
  fire: () => {
    tone(95, 0.14, 0.045, 55);
    setTimeout(() => tone(250, 0.08, 0.018, 100), 85);
  },
  miss: () => tone(170, 0.22, 0.025, 90),
  hit: () => tone(110, 0.28, 0.06, 45),
  sunk: () => {
    tone(150, 0.38, 0.06, 42);
    setTimeout(() => tone(80, 0.44, 0.04, 38), 120);
  },
  turn: () => tone(560, 0.12, 0.025, 760),
  ready: () => tone(610, 0.16, 0.025, 790),
  start: () => {
    [220, 330, 520].forEach((frequency, index) =>
      setTimeout(() => tone(frequency, 0.18, 0.028, frequency * 1.12), index * 100)
    );
  },
  countdown: (seconds: number) =>
    tone(seconds <= 3 ? 740 : seconds <= 5 ? 660 : 590, 0.075, 0.018, 520),
  victory: () => {
    [440, 554, 659].forEach((frequency, index) =>
      setTimeout(() => tone(frequency, 0.35, 0.035), index * 130)
    );
  },
  defeat: () => {
    tone(160, 0.42, 0.04, 62);
    setTimeout(() => tone(90, 0.48, 0.028, 42), 180);
  }
};
