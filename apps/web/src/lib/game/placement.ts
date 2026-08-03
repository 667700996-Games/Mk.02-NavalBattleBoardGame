import { FLEET, coordinateKey, type Coordinate, type Orientation, type ShipKind, type ShipPlacement } from '$lib/types';

export interface PlacementValidation {
  valid: boolean;
  cells: Coordinate[];
  reason?: 'OUT_OF_BOUNDS' | 'OVERLAP' | 'INCOMPLETE' | 'DUPLICATE';
}

export function cellsForPlacement(placement: ShipPlacement): Coordinate[] {
  const size = FLEET.find((ship) => ship.kind === placement.kind)?.size ?? 0;
  return Array.from({ length: size }, (_, offset) => ({
    row: placement.origin.row + (placement.orientation === 'VERTICAL' ? offset : 0),
    col: placement.origin.col + (placement.orientation === 'HORIZONTAL' ? offset : 0)
  }));
}

export function validatePlacement(
  candidate: ShipPlacement,
  placements: ShipPlacement[]
): PlacementValidation {
  const cells = cellsForPlacement(candidate);
  if (cells.some((cell) => cell.row < 0 || cell.row >= 10 || cell.col < 0 || cell.col >= 10)) {
    return { valid: false, cells, reason: 'OUT_OF_BOUNDS' };
  }
  const occupied = new Set(
    placements
      .filter((placement) => placement.kind !== candidate.kind)
      .flatMap(cellsForPlacement)
      .map(coordinateKey)
  );
  if (cells.some((cell) => occupied.has(coordinateKey(cell)))) {
    return { valid: false, cells, reason: 'OVERLAP' };
  }
  return { valid: true, cells };
}

export function validateFleet(placements: ShipPlacement[]): PlacementValidation {
  if (placements.length !== FLEET.length) {
    return { valid: false, cells: [], reason: 'INCOMPLETE' };
  }
  if (new Set(placements.map((placement) => placement.kind)).size !== FLEET.length) {
    return { valid: false, cells: [], reason: 'DUPLICATE' };
  }
  for (const placement of placements) {
    const validation = validatePlacement(placement, placements);
    if (!validation.valid) return validation;
  }
  return { valid: true, cells: placements.flatMap(cellsForPlacement) };
}

export function rotatePlacement(placement: ShipPlacement): ShipPlacement {
  return {
    ...placement,
    orientation: placement.orientation === 'HORIZONTAL' ? 'VERTICAL' : 'HORIZONTAL'
  };
}

export function autoPlaceFleet(random: () => number = Math.random): ShipPlacement[] {
  const placements: ShipPlacement[] = [];
  for (const ship of FLEET) {
    let placed = false;
    for (let attempt = 0; attempt < 1_000; attempt += 1) {
      const orientation: Orientation = random() < 0.5 ? 'HORIZONTAL' : 'VERTICAL';
      const maxRow = orientation === 'VERTICAL' ? 10 - ship.size : 9;
      const maxCol = orientation === 'HORIZONTAL' ? 10 - ship.size : 9;
      const candidate: ShipPlacement = {
        kind: ship.kind,
        orientation,
        origin: {
          row: Math.floor(random() * (maxRow + 1)),
          col: Math.floor(random() * (maxCol + 1))
        }
      };
      if (validatePlacement(candidate, placements).valid) {
        placements.push(candidate);
        placed = true;
        break;
      }
    }
    if (!placed) return autoPlaceFleet(random);
  }
  return placements;
}

export function placementAt(placements: ShipPlacement[], coordinate: Coordinate): ShipKind | null {
  return (
    placements.find((placement) =>
      cellsForPlacement(placement).some(
        (cell) => cell.row === coordinate.row && cell.col === coordinate.col
      )
    )?.kind ?? null
  );
}

