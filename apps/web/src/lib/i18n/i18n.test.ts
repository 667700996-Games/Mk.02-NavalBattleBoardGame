import { afterEach, beforeAll, describe, expect, it } from 'vitest';

import enUS from './messages/en-US.json';
import koKR from './messages/ko-KR.json';
import {
  formatDateTime,
  formatNumber,
  formatRelativeTime,
  localizeError,
  loadLocaleCatalog,
  pseudoLocalize,
  setLocale,
  translate,
  type MessageKey
} from './index';

beforeAll(() => loadLocaleCatalog('en-US'));
afterEach(() => setLocale('ko-KR'));

describe('localization runtime', () => {
  it('renders Korean and English from one stable key', () => {
    expect(translate('ko-KR', 'landing.enterLobby')).toBe('작전 로비 입장');
    expect(translate('en-US', 'landing.enterLobby')).toBe('Enter operations lobby');
  });

  it('preserves interpolation tokens while expanding pseudolocalized copy', () => {
    const source = translate('en-US', 'layout.currentTime', { time: '12:30' });
    const pseudo = translate('en-XA', 'layout.currentTime', { time: '12:30' });
    expect(pseudo).toContain('12:30');
    expect(pseudo.length).toBeGreaterThan(source.length * 1.25);
  });

  it('expands every launch message by at least 25 percent in the pseudo locale', () => {
    for (const key of Object.keys(enUS) as MessageKey[]) {
      const source = translate('en-US', key);
      const pseudo = translate('en-XA', key);
      expect(pseudo.length, key).toBeGreaterThanOrEqual(Math.ceil(source.length * 1.25));
    }
  });

  it('keeps Korean and English catalogs structurally identical', () => {
    expect(Object.keys(enUS).sort()).toEqual(Object.keys(koKR).sort());
  });

  it('formats locale-aware dates and numbers without hard-coded separators', async () => {
    await setLocale('en-US');
    expect(formatNumber(1234567.5)).toBe('1,234,567.5');
    expect(
      formatDateTime('2026-08-18T12:30:00.000Z', {
        dateStyle: 'long',
        timeZone: 'UTC'
      })
    ).toContain('August');
    expect(formatRelativeTime(-1, 'day')).toBe('yesterday');

    await setLocale('ko-KR');
    expect(
      formatDateTime('2026-08-18T12:30:00.000Z', {
        dateStyle: 'long',
        timeZone: 'UTC'
      })
    ).toContain('8월');
    expect(formatRelativeTime(-1, 'day')).toBe('어제');
  });

  it('does not alter placeholders in the standalone pseudolocalizer', () => {
    expect(pseudoLocalize('Target {name}')).toContain('{name}');
  });

  it('localizes structured server errors without exposing server-language copy', async () => {
    await setLocale('en-US');
    expect(
      localizeError(
        { code: 'ROOM_FULL', message: '이미 두 명이 참가한 방입니다.' },
        'error.requestFailed'
      )
    ).toBe('Two players have already joined this room.');
    expect(localizeError(new Error('internal detail'), 'error.requestFailed')).toBe(
      'The request could not be completed.'
    );
  });
});
