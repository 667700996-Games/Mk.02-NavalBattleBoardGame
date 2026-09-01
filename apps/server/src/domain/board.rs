use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::GameError;

use super::{BOARD_SIZE, BalanceManifest, BalancePin, Coordinate};

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
        self.cells_for(&BalancePin::current().manifest)
    }

    pub fn cells_for(&self, balance: &BalanceManifest) -> Result<Vec<Coordinate>, GameError> {
        let size = balance
            .ship_cells(self.kind)
            .ok_or(GameError::InvalidFleetComposition)?;
        let mut cells = Vec::with_capacity(size as usize);
        for offset in 0..size {
            let (row, col) = match self.orientation {
                Orientation::Horizontal => (self.origin.row, self.origin.col + offset),
                Orientation::Vertical => (self.origin.row + offset, self.origin.col),
            };
            if row >= balance.board_size || col >= balance.board_size {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    #[serde(default = "default_board_size")]
    board_size: u8,
    ships: Vec<Ship>,
    attacks_received: Vec<ReceivedAttack>,
}

impl Default for Board {
    fn default() -> Self {
        Self {
            board_size: BOARD_SIZE,
            ships: Vec::new(),
            attacks_received: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceivedAttack {
    pub coordinate: Coordinate,
    pub outcome: AttackOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardAttackResult {
    pub outcome: AttackOutcome,
    pub sunk_ship: Option<ShipKind>,
    pub all_sunk: bool,
}

impl Board {
    pub fn from_placements(placements: &[ShipPlacement]) -> Result<Self, GameError> {
        Self::from_placements_for(placements, &BalancePin::current().manifest)
    }

    pub fn from_placements_for(
        placements: &[ShipPlacement],
        balance: &BalanceManifest,
    ) -> Result<Self, GameError> {
        if !balance.has_valid_shape() || placements.len() != balance.fleet.len() {
            return Err(GameError::IncompleteFleet);
        }

        let kinds: HashSet<_> = placements.iter().map(|ship| ship.kind).collect();
        if kinds.len() != balance.fleet.len()
            || !balance.fleet.iter().all(|ship| kinds.contains(&ship.kind))
        {
            return Err(GameError::InvalidFleetComposition);
        }

        let mut occupied = HashSet::new();
        let mut ships = Vec::with_capacity(placements.len());
        for placement in placements {
            let cells = placement.cells_for(balance)?;
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
            board_size: balance.board_size,
            ships,
            attacks_received: Vec::new(),
        })
    }

    pub fn attack(&mut self, coordinate: Coordinate) -> Result<BoardAttackResult, GameError> {
        Coordinate::new_for_board(coordinate.row, coordinate.col, self.board_size)?;
        if self
            .attacks_received
            .iter()
            .any(|attack| attack.coordinate == coordinate)
        {
            return Err(GameError::CoordinateAlreadyAttacked);
        }

        let mut outcome = AttackOutcome::Miss;
        let mut sunk_ship = None;
        if let Some(ship) = self
            .ships
            .iter_mut()
            .find(|ship| ship.cells.contains(&coordinate))
        {
            ship.hits.insert(coordinate);
            if ship.is_sunk() {
                outcome = AttackOutcome::Sunk;
                sunk_ship = Some(ship.kind);
            } else {
                outcome = AttackOutcome::Hit;
            }
        }
        self.attacks_received.push(ReceivedAttack {
            coordinate,
            outcome,
        });

        Ok(BoardAttackResult {
            outcome,
            sunk_ship,
            all_sunk: self.ships.iter().all(Ship::is_sunk),
        })
    }

    pub fn ships(&self) -> &[Ship] {
        &self.ships
    }

    pub fn attacks_received(&self) -> &[ReceivedAttack] {
        &self.attacks_received
    }

    pub fn was_attacked(&self, coordinate: Coordinate) -> bool {
        self.attacks_received
            .iter()
            .any(|attack| attack.coordinate == coordinate)
    }
}

const fn default_board_size() -> u8 {
    BOARD_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fleet() -> Vec<ShipPlacement> {
        vec![
            ShipPlacement {
                kind: ShipKind::Carrier,
                origin: Coordinate { row: 0, col: 0 },
                orientation: Orientation::Horizontal,
            },
            ShipPlacement {
                kind: ShipKind::Battleship,
                origin: Coordinate { row: 2, col: 0 },
                orientation: Orientation::Horizontal,
            },
            ShipPlacement {
                kind: ShipKind::Cruiser,
                origin: Coordinate { row: 4, col: 0 },
                orientation: Orientation::Horizontal,
            },
            ShipPlacement {
                kind: ShipKind::Submarine,
                origin: Coordinate { row: 6, col: 0 },
                orientation: Orientation::Horizontal,
            },
            ShipPlacement {
                kind: ShipKind::Destroyer,
                origin: Coordinate { row: 8, col: 0 },
                orientation: Orientation::Horizontal,
            },
        ]
    }

    #[test]
    fn creates_a_valid_fleet() {
        let board = Board::from_placements(&fleet()).unwrap();
        assert_eq!(board.ships().len(), 5);
        assert_eq!(
            board.ships().iter().map(|s| s.cells.len()).sum::<usize>(),
            17
        );
    }

    #[test]
    fn a_self_contained_manifest_controls_board_and_ship_dimensions() {
        let mut balance = BalanceManifest::v1();
        balance.board_size = 8;
        balance.fleet[0].cells = 4;
        let placements = [
            (ShipKind::Carrier, 0),
            (ShipKind::Battleship, 1),
            (ShipKind::Cruiser, 2),
            (ShipKind::Submarine, 3),
            (ShipKind::Destroyer, 4),
        ]
        .map(|(kind, row)| ShipPlacement {
            kind,
            origin: Coordinate { row, col: 0 },
            orientation: Orientation::Horizontal,
        });
        let mut board = Board::from_placements_for(&placements, &balance).unwrap();
        assert_eq!(
            board
                .ships()
                .iter()
                .map(|ship| ship.cells.len())
                .sum::<usize>(),
            16
        );
        assert_eq!(
            board.attack(Coordinate { row: 8, col: 0 }).unwrap_err(),
            GameError::InvalidCoordinate
        );
    }

    #[test]
    fn rejects_overlap() {
        let mut placements = fleet();
        placements[1].origin = Coordinate { row: 0, col: 2 };
        assert_eq!(
            Board::from_placements(&placements).unwrap_err(),
            GameError::ShipsOverlap
        );
    }

    #[test]
    fn rejects_out_of_bounds_in_both_directions() {
        let horizontal = ShipPlacement {
            kind: ShipKind::Carrier,
            origin: Coordinate { row: 0, col: 6 },
            orientation: Orientation::Horizontal,
        };
        let vertical = ShipPlacement {
            kind: ShipKind::Carrier,
            origin: Coordinate { row: 6, col: 0 },
            orientation: Orientation::Vertical,
        };
        assert_eq!(
            horizontal.cells().unwrap_err(),
            GameError::PlacementOutOfBounds
        );
        assert_eq!(
            vertical.cells().unwrap_err(),
            GameError::PlacementOutOfBounds
        );
    }

    #[test]
    fn calculates_hit_sink_and_win() {
        let mut board = Board::from_placements(&fleet()).unwrap();
        assert_eq!(
            board.attack(Coordinate { row: 9, col: 9 }).unwrap().outcome,
            AttackOutcome::Miss
        );
        assert_eq!(
            board.attack(Coordinate { row: 8, col: 0 }).unwrap().outcome,
            AttackOutcome::Hit
        );
        let sunk = board.attack(Coordinate { row: 8, col: 1 }).unwrap();
        assert_eq!(sunk.outcome, AttackOutcome::Sunk);
        assert_eq!(sunk.sunk_ship, Some(ShipKind::Destroyer));
        assert!(!sunk.all_sunk);
    }

    #[test]
    fn rejects_duplicate_attack() {
        let mut board = Board::from_placements(&fleet()).unwrap();
        board.attack(Coordinate { row: 9, col: 9 }).unwrap();
        assert_eq!(
            board.attack(Coordinate { row: 9, col: 9 }).unwrap_err(),
            GameError::CoordinateAlreadyAttacked
        );
    }
}
