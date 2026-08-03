import { describe, expect, it } from 'vitest';
import { autoPlaceFleet, cellsForPlacement, validateFleet, validatePlacement } from './placement';
import type { ShipPlacement } from '$lib/types';

describe('fleet placement', () => {
  it('calculates horizontal and vertical cells', () => {
    expect(
      cellsForPlacement({
        kind: 'DESTROYER',
        origin: { row: 3, col: 4 },
        orientation: 'HORIZONTAL'
      })
    ).toEqual([
      { row: 3, col: 4 },
      { row: 3, col: 5 }
    ]);
    expect(
      cellsForPlacement({
        kind: 'DESTROYER',
        origin: { row: 3, col: 4 },
        orientation: 'VERTICAL'
      })
    ).toEqual([
      { row: 3, col: 4 },
      { row: 4, col: 4 }
    ]);
  });

  it('rejects overlap and boundary overflow', () => {
    const carrier: ShipPlacement = {
      kind: 'CARRIER',
      origin: { row: 0, col: 0 },
      orientation: 'HORIZONTAL'
    };
    expect(
      validatePlacement(
        { kind: 'DESTROYER', origin: { row: 0, col: 3 }, orientation: 'VERTICAL' },
        [carrier]
      ).reason
    ).toBe('OVERLAP');
    expect(
      validatePlacement(
        { kind: 'CARRIER', origin: { row: 8, col: 0 }, orientation: 'VERTICAL' },
        []
      ).reason
    ).toBe('OUT_OF_BOUNDS');
  });

  it('creates a complete valid fleet repeatedly', () => {
    for (let index = 0; index < 100; index += 1) {
      expect(validateFleet(autoPlaceFleet()).valid).toBe(true);
    }
  });
});
