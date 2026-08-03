use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::error::GameError;

use super::{BOARD_SIZE, Coordinate};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ShipKind {
    Carrier,
    Battleship,
    Cruiser,
    Submarine,
    Destroyer,
}

impl ShipKind {
    pub const ALL: [Self; 5] = [
        Self::Carrier,
        Self::Battleship,
        Self::Cruiser,
        Self::Submarine,
        Self::Destroyer,
    ];

    pub const fn size(self) -> u8 {
        match self {
            Self::Carrier => 5,
            Self::Battleship => 4,
            Self::Cruiser | Self::Submarine => 3,
            Self::Destroyer => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShipPlacement {
    pub kind: ShipKind,
    pub origin: Coordinate,
    pub orientation: Orientation,
}

impl ShipPlacement {
    pub fn cells(&self) -> Result<Vec<Coordinate>, GameError> {
        let mut cells = Vec::with_capacity(self.kind.size() as usize);
        for offset in 0..self.kind.size() {
            let (row, col) = match self.orientation {
                Orientation::Horizontal => (self.origin.row, self.origin.col + offset),
                Orientation::Vertical => (self.origin.row + offset, self.origin.col),
            };
            if row >= BOARD_SIZE || col >= BOARD_SIZE {
                return Err(GameError::PlacementOutOfBounds);
            }
            cells.push(Coordinate { row, col });
        }
        Ok(cells)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ship {
    pub kind: ShipKind,
    pub cells: Vec<Coordinate>,
    pub hits: HashSet<Coordinate>,
}

impl Ship {
    pub fn is_sunk(&self) -> bool {
        self.cells.iter().all(|cell| self.hits.contains(cell))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AttackOutcome {
    Miss,
    Hit,
    Sunk,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    ships: Vec<Ship>,
    attacks_received: HashMap<Coordinate, AttackOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardAttackResult {
    pub outcome: AttackOutcome,
    pub sunk_ship: Option<ShipKind>,
    pub all_sunk: bool,
}

impl Board {
    pub fn from_placements(placements: &[ShipPlacement]) -> Result<Self, GameError> {
        if placements.len() != ShipKind::ALL.len() {
            return Err(GameError::IncompleteFleet);
        }

        let kinds: HashSet<_> = placements.iter().map(|ship| ship.kind).collect();
        if kinds.len() != ShipKind::ALL.len()
            || !ShipKind::ALL.iter().all(|kind| kinds.contains(kind))
        {
            return Err(GameError::InvalidFleetComposition);
        }

        let mut occupied = HashSet::new();
        let mut ships = Vec::with_capacity(placements.len());
        for placement in placements {
            let cells = placement.cells()?;
            if cells.iter().any(|cell| !occupied.insert(*cell)) {
                return Err(GameError::ShipsOverlap);
            }
            ships.push(Ship {
                kind: placement.kind,
                cells,
                hits: HashSet::new(),
            });
        }

        Ok(Self {
            ships,
            attacks_received: HashMap::new(),
        })
    }

    pub fn attack(&mut self, coordinate: Coordinate) -> Result<BoardAttackResult, GameError> {
        Coordinate::new(coordinate.row, coordinate.col)?;
        if self.attacks_received.contains_key(&coordinate) {
            return Err(GameError::CoordinateAlreadyAttacked);
        }

        let mut outcome = AttackOutcome::Miss;
        let mut sunk_ship = None;
        if let Some(ship) = self.ships.iter_mut().find(|ship| ship.cells.contains(&coordinate)) {
            ship.hits.insert(coordinate);
            if ship.is_sunk() {
                outcome = AttackOutcome::Sunk;
                sunk_ship = Some(ship.kind);
            } else {
                outcome = AttackOutcome::Hit;
            }
        }
        self.attacks_received.insert(coordinate, outcome);

        Ok(BoardAttackResult {
            outcome,
            sunk_ship,
            all_sunk: self.ships.iter().all(Ship::is_sunk),
        })
    }

    pub fn ships(&self) -> &[Ship] {
        &self.ships
    }

    pub fn attacks_received(&self) -> &HashMap<Coordinate, AttackOutcome> {
        &self.attacks_received
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fleet() -> Vec<ShipPlacement> {
        vec![
            ShipPlacement { kind: ShipKind::Carrier, origin: Coordinate { row: 0, col: 0 }, orientation: Orientation::Horizontal },
            ShipPlacement { kind: ShipKind::Battleship, origin: Coordinate { row: 2, col: 0 }, orientation: Orientation::Horizontal },
            ShipPlacement { kind: ShipKind::Cruiser, origin: Coordinate { row: 4, col: 0 }, orientation: Orientation::Horizontal },
            ShipPlacement { kind: ShipKind::Submarine, origin: Coordinate { row: 6, col: 0 }, orientation: Orientation::Horizontal },
            ShipPlacement { kind: ShipKind::Destroyer, origin: Coordinate { row: 8, col: 0 }, orientation: Orientation::Horizontal },
        ]
    }

    #[test]
    fn creates_a_valid_fleet() {
        let board = Board::from_placements(&fleet()).unwrap();
        assert_eq!(board.ships().len(), 5);
        assert_eq!(board.ships().iter().map(|s| s.cells.len()).sum::<usize>(), 17);
    }

    #[test]
    fn rejects_overlap() {
        let mut placements = fleet();
        placements[1].origin = Coordinate { row: 0, col: 2 };
        assert_eq!(Board::from_placements(&placements).unwrap_err(), GameError::ShipsOverlap);
    }

    #[test]
    fn rejects_out_of_bounds_in_both_directions() {
        let horizontal = ShipPlacement { kind: ShipKind::Carrier, origin: Coordinate { row: 0, col: 6 }, orientation: Orientation::Horizontal };
        let vertical = ShipPlacement { kind: ShipKind::Carrier, origin: Coordinate { row: 6, col: 0 }, orientation: Orientation::Vertical };
        assert_eq!(horizontal.cells().unwrap_err(), GameError::PlacementOutOfBounds);
        assert_eq!(vertical.cells().unwrap_err(), GameError::PlacementOutOfBounds);
    }

    #[test]
    fn calculates_hit_sink_and_win() {
        let mut board = Board::from_placements(&fleet()).unwrap();
        assert_eq!(board.attack(Coordinate { row: 9, col: 9 }).unwrap().outcome, AttackOutcome::Miss);
        assert_eq!(board.attack(Coordinate { row: 8, col: 0 }).unwrap().outcome, AttackOutcome::Hit);
        let sunk = board.attack(Coordinate { row: 8, col: 1 }).unwrap();
        assert_eq!(sunk.outcome, AttackOutcome::Sunk);
        assert_eq!(sunk.sunk_ship, Some(ShipKind::Destroyer));
        assert!(!sunk.all_sunk);
    }

    #[test]
    fn rejects_duplicate_attack() {
        let mut board = Board::from_placements(&fleet()).unwrap();
        board.attack(Coordinate { row: 9, col: 9 }).unwrap();
        assert_eq!(board.attack(Coordinate { row: 9, col: 9 }).unwrap_err(), GameError::CoordinateAlreadyAttacked);
    }
}
