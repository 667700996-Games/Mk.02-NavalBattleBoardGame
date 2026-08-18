import {
  coordinateKey,
  fleetForBalance,
  type BalanceManifest,
  type Coordinate,
  type Orientation,
  type ShipKind,
  type ShipPlacement
} from '$lib/types';

export interface PlacementValidation {
  valid: boolean;
  cells: Coordinate[];
  reason?: 'OUT_OF_BOUNDS' | 'OVERLAP' | 'INCOMPLETE' | 'DUPLICATE';
}

export function cellsForPlacement(
  placement: ShipPlacement,
  balance?: BalanceManifest | null
): Coordinate[] {
  const size = fleetForBalance(balance).find((ship) => ship.kind === placement.kind)?.size ?? 0;
  return Array.from({ length: size }, (_, offset) => ({
    row: placement.origin.row + (placement.orientation === 'VERTICAL' ? offset : 0),
    col: placement.origin.col + (placement.orientation === 'HORIZONTAL' ? offset : 0)
  }));
}

export function validatePlacement(
  candidate: ShipPlacement,
  placements: ShipPlacement[],
  balance?: BalanceManifest | null
): PlacementValidation {
  const boardSize = balance?.boardSize ?? 10;
  const cells = cellsForPlacement(candidate, balance);
  if (
    cells.some(
      (cell) => cell.row < 0 || cell.row >= boardSize || cell.col < 0 || cell.col >= boardSize
    )
  ) {
    return { valid: false, cells, reason: 'OUT_OF_BOUNDS' };
  }
  const occupied = new Set(
    placements
      .filter((placement) => placement.kind !== candidate.kind)
      .flatMap((placement) => cellsForPlacement(placement, balance))
      .map(coordinateKey)
  );
  if (cells.some((cell) => occupied.has(coordinateKey(cell)))) {
    return { valid: false, cells, reason: 'OVERLAP' };
  }
  return { valid: true, cells };
}

export function validateFleet(
  placements: ShipPlacement[],
  balance?: BalanceManifest | null
): PlacementValidation {
  const fleet = fleetForBalance(balance);
  if (placements.length !== fleet.length) {
    return { valid: false, cells: [], reason: 'INCOMPLETE' };
  }
  if (new Set(placements.map((placement) => placement.kind)).size !== fleet.length) {
    return { valid: false, cells: [], reason: 'DUPLICATE' };
  }
  for (const placement of placements) {
    const validation = validatePlacement(placement, placements, balance);
    if (!validation.valid) return validation;
  }
  return {
    valid: true,
    cells: placements.flatMap((placement) => cellsForPlacement(placement, balance))
  };
}

export function rotatePlacement(placement: ShipPlacement): ShipPlacement {
  return {
    ...placement,
    orientation: placement.orientation === 'HORIZONTAL' ? 'VERTICAL' : 'HORIZONTAL'
  };
}

export function autoPlaceFleet(
  random: () => number = Math.random,
  balance?: BalanceManifest | null
): ShipPlacement[] {
  const placements: ShipPlacement[] = [];
  const boardSize = balance?.boardSize ?? 10;
  for (const ship of fleetForBalance(balance)) {
    let placed = false;
    for (let attempt = 0; attempt < 1_000; attempt += 1) {
      const orientation: Orientation = random() < 0.5 ? 'HORIZONTAL' : 'VERTICAL';
      const maxRow = orientation === 'VERTICAL' ? boardSize - ship.size : boardSize - 1;
      const maxCol = orientation === 'HORIZONTAL' ? boardSize - ship.size : boardSize - 1;
      const candidate: ShipPlacement = {
        kind: ship.kind,
        orientation,
        origin: {
          row: Math.floor(random() * (maxRow + 1)),
          col: Math.floor(random() * (maxCol + 1))
        }
      };
      if (validatePlacement(candidate, placements, balance).valid) {
        placements.push(candidate);
        placed = true;
        break;
      }
    }
    if (!placed) return autoPlaceFleet(random, balance);
  }
  return placements;
}

export function placementAt(
  placements: ShipPlacement[],
  coordinate: Coordinate,
  balance?: BalanceManifest | null
): ShipKind | null {
  return (
    placements.find((placement) =>
      cellsForPlacement(placement, balance).some(
        (cell) => cell.row === coordinate.row && cell.col === coordinate.col
      )
    )?.kind ?? null
  );
}
