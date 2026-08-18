import { describe, expect, it } from 'vitest';
import { autoPlaceFleet, cellsForPlacement, validateFleet, validatePlacement } from './placement';
import type { BalanceManifest, ShipPlacement } from '$lib/types';

const compactBalance: BalanceManifest = {
  schemaVersion: 1,
  rulesetVersion: 99,
  label: 'Compact fixture',
  boardSize: 8,
  fleet: [
    { kind: 'CARRIER', cells: 4 },
    { kind: 'BATTLESHIP', cells: 3 },
    { kind: 'CRUISER', cells: 3 },
    { kind: 'SUBMARINE', cells: 2 },
    { kind: 'DESTROYER', cells: 2 }
  ],
  classicShotsPerTurn: 1,
  rapidTurnDurationSeconds: 20,
  maximumTurnDurationSeconds: 180,
  consecutiveTimeoutForfeit: 4,
  salvoShotPolicy: 'SURVIVING_SHIPS',
  turnAdvancePolicy: 'AFTER_SHOT_ALLOWANCE',
  duplicateTargetPolicy: 'REJECT',
  victoryCondition: 'SINK_ALL_SHIPS',
  fleetRevealPolicy: 'MATCH_COMPLETE'
};

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

  it('uses the pinned manifest instead of current fleet and board constants', () => {
    expect(
      cellsForPlacement(
        {
          kind: 'CARRIER',
          origin: { row: 0, col: 4 },
          orientation: 'HORIZONTAL'
        },
        compactBalance
      )
    ).toHaveLength(4);
    expect(
      validatePlacement(
        { kind: 'CARRIER', origin: { row: 0, col: 5 }, orientation: 'HORIZONTAL' },
        [],
        compactBalance
      ).reason
    ).toBe('OUT_OF_BOUNDS');
    for (let index = 0; index < 50; index += 1) {
      expect(validateFleet(autoPlaceFleet(Math.random, compactBalance), compactBalance).valid).toBe(
        true
      );
    }
  });
});
