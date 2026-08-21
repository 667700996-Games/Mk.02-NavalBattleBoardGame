import { describe, expect, it } from 'vitest';
import { classifyRumRoute } from './performance';

describe('RUM route classification', () => {
  it.each([
    ['/', 'landing'],
    ['/tutorial', 'tutorial'],
    ['/play', 'lobby'],
    ['/single-player', 'lobby'],
    ['/lobby', 'lobby'],
    ['/join/ABCD12', 'join'],
    ['/room/ABCD12', 'room'],
    ['/settings', 'account'],
    ['/stats', 'account'],
    ['/replay/room-id', 'replay'],
    ['/admin/moderation', 'other'],
    ['/unexpected/player-identifier', 'other']
  ] as const)('maps %s to the bounded %s label', (pathname, expected) => {
    expect(classifyRumRoute(pathname)).toBe(expected);
  });
});
