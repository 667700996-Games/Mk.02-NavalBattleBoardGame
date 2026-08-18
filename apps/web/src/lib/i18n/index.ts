import { browser } from '$app/environment';
import { derived, get, writable } from 'svelte/store';

import koKR from './messages/ko-KR.json';
import type {
  GameMode,
  MatchmakingRegion,
  MatchmakingSearchPhase,
  QuickCommandId,
  RoomStatus,
  ShipKind
} from '$lib/types';

export const launchLocales = ['ko-KR', 'en-US'] as const;
export const testLocales = ['en-XA'] as const;
export type LaunchLocale = (typeof launchLocales)[number];
export type Locale = LaunchLocale | (typeof testLocales)[number];
export type MessageKey = keyof typeof koKR;
export type MessageValues = Record<string, string | number>;
export type Translator = (key: MessageKey, values?: MessageValues) => string;

const catalogs: Partial<Record<LaunchLocale, Record<MessageKey, string>>> = {
  'ko-KR': koKR
};
let englishCatalogPromise: Promise<Record<MessageKey, string>> | null = null;
let localeRequest = 0;

const accents: Record<string, string> = {
  a: 'å',
  b: 'ƀ',
  c: 'ç',
  d: 'ð',
  e: 'ë',
  f: 'ƒ',
  g: 'ğ',
  h: 'ħ',
  i: 'ï',
  j: 'ĵ',
  k: 'ķ',
  l: 'ļ',
  m: 'ɱ',
  n: 'ñ',
  o: 'ø',
  p: 'þ',
  q: 'ɋ',
  r: 'ŕ',
  s: 'š',
  t: 'ţ',
  u: 'ü',
  v: 'ṽ',
  w: 'ŵ',
  x: 'ẋ',
  y: 'ÿ',
  z: 'ž'
};

function isLocale(value: string | null): value is Locale {
  return [...launchLocales, ...testLocales].includes(value as Locale);
}

export function pseudoLocalize(source: string): string {
  const protectedParts = source.split(/(\{[a-zA-Z][a-zA-Z0-9]*\})/g);
  const transformed = protectedParts
    .map((part) => {
      if (/^\{[a-zA-Z][a-zA-Z0-9]*\}$/.test(part)) return part;
      return [...part]
        .map((character) => {
          const replacement = accents[character.toLowerCase()];
          if (!replacement) return character;
          return character === character.toUpperCase() ? replacement.toUpperCase() : replacement;
        })
        .join('');
    })
    .join('');
  const padding = '~'.repeat(Math.max(4, Math.ceil(source.length * 0.35)));
  return `⟦${transformed} ${padding}⟧`;
}

function interpolate(message: string, values: MessageValues = {}): string {
  return message.replace(/\{([a-zA-Z][a-zA-Z0-9]*)\}/g, (placeholder, name: string) =>
    Object.hasOwn(values, name) ? String(values[name]) : placeholder
  );
}

export function translate(locale: Locale, key: MessageKey, values: MessageValues = {}): string {
  const catalog = catalogs[locale === 'en-XA' ? 'en-US' : locale] ?? catalogs['ko-KR'];
  const source = catalog![key];
  return interpolate(locale === 'en-XA' ? pseudoLocalize(source) : source, values);
}

export function message(key: MessageKey, values?: MessageValues): string {
  return translate(get(locale), key, values);
}

export function localizeError(
  error: unknown,
  fallbackKey: MessageKey,
  includeCode = false
): string {
  const code =
    typeof error === 'object' && error !== null && 'code' in error && typeof error.code === 'string'
      ? error.code
      : null;
  const candidate = code ? (`apiError.${code}` as MessageKey) : null;
  const key = candidate && Object.hasOwn(catalogs['ko-KR']!, candidate) ? candidate : fallbackKey;
  const localized = message(key);
  return includeCode && code ? `${localized} (${code})` : localized;
}

export const locale = writable<Locale>('ko-KR');
export const t = derived(
  locale,
  ($locale) => (key: MessageKey, values?: MessageValues) => translate($locale, key, values)
);

function persistLocale(value: Locale): void {
  if (!browser) return;
  localStorage.setItem('mk01_locale', value);
  document.cookie = `mk01_locale=${encodeURIComponent(value)}; Path=/; Max-Age=31536000; SameSite=Lax`;
  document.documentElement.lang = value === 'en-XA' ? 'en' : value;
  document.documentElement.dataset.locale = value;
}

export async function loadLocaleCatalog(value: Locale): Promise<void> {
  if (value === 'ko-KR' || catalogs['en-US']) return;
  englishCatalogPromise ??= import('./messages/en-US.json').then(
    (module) => module.default as Record<MessageKey, string>
  );
  catalogs['en-US'] = await englishCatalogPromise;
}

export async function setLocale(value: Locale): Promise<void> {
  const request = ++localeRequest;
  await loadLocaleCatalog(value);
  if (request !== localeRequest) return;
  locale.set(value);
  persistLocale(value);
}

export async function initializeLocale(): Promise<void> {
  if (!browser) return;
  const cookie = document.cookie
    .split('; ')
    .find((entry) => entry.startsWith('mk01_locale='))
    ?.split('=')[1];
  const stored = localStorage.getItem('mk01_locale');
  const requested = stored ?? (cookie ? decodeURIComponent(cookie) : null);
  const preferred = isLocale(requested)
    ? requested
    : navigator.language.toLowerCase().startsWith('en')
      ? 'en-US'
      : 'ko-KR';
  await setLocale(preferred);
}

function intlLocale(value: Locale): LaunchLocale {
  return value === 'en-XA' ? 'en-US' : value;
}

export function formatDateTime(
  value: string | number | Date,
  options: Intl.DateTimeFormatOptions = { dateStyle: 'medium', timeStyle: 'short' }
): string {
  return new Intl.DateTimeFormat(intlLocale(get(locale)), options).format(new Date(value));
}

export function formatNumber(value: number, options?: Intl.NumberFormatOptions): string {
  return new Intl.NumberFormat(intlLocale(get(locale)), options).format(value);
}

export function formatRelativeTime(
  value: number,
  unit: Intl.RelativeTimeFormatUnit,
  options: Intl.RelativeTimeFormatOptions = { numeric: 'auto' }
): string {
  return new Intl.RelativeTimeFormat(intlLocale(get(locale)), options).format(value, unit);
}

export function shipMessageKey(kind: ShipKind): MessageKey {
  return `ship.${kind}` as MessageKey;
}

export function shipName(kind: ShipKind, targetLocale: Locale = get(locale)): string {
  return translate(targetLocale, shipMessageKey(kind));
}

export function quickCommandMessageKey(command: QuickCommandId): MessageKey {
  return `quickCommand.${command}` as MessageKey;
}

export function quickCommandLabel(
  command: QuickCommandId,
  targetLocale: Locale = get(locale)
): string {
  return translate(targetLocale, quickCommandMessageKey(command));
}

export function gameModeMessageKey(mode: GameMode): MessageKey {
  return `gameMode.${mode}` as MessageKey;
}

export function roomStatusMessageKey(status: RoomStatus): MessageKey {
  return `roomStatus.${status}` as MessageKey;
}

export function matchPhaseMessageKey(phase: MatchmakingSearchPhase): MessageKey {
  return `matchPhase.${phase}` as MessageKey;
}

export function regionMessageKey(region: Exclude<MatchmakingRegion, 'AUTO'>): MessageKey {
  return `region.${region}` as MessageKey;
}
