use std::collections::HashSet;

use mk01_server::{
    domain::{AttackOutcome, BalancePin, Board, Coordinate, Orientation, ShipKind, ShipPlacement},
    error::GameError,
};
use rand::{Rng, SeedableRng, rngs::StdRng, seq::SliceRandom};

// This count is mirrored by config/quality-gates.json and enforced by the quality-policy checker.
const PROPERTY_CASES: u64 = 4_096;
const PROPERTY_SEED: u64 = 0x4d4b_3031_4141_4102;

fn canonical_fleet() -> Vec<ShipPlacement> {
    [
        (ShipKind::Carrier, 0),
        (ShipKind::Battleship, 2),
        (ShipKind::Cruiser, 4),
        (ShipKind::Submarine, 6),
        (ShipKind::Destroyer, 8),
    ]
    .into_iter()
    .map(|(kind, row)| ShipPlacement {
        kind,
        origin: Coordinate { row, col: 0 },
        orientation: Orientation::Horizontal,
    })
    .collect()
}

#[test]
fn generated_placements_are_contiguous_unique_and_bounded_or_rejected() {
    let balance = BalancePin::current().manifest;
    let mut rng = StdRng::seed_from_u64(PROPERTY_SEED);

    for case in 0..PROPERTY_CASES {
        let kind = ShipKind::ALL[rng.random_range(0..ShipKind::ALL.len())];
        let orientation = if rng.random_bool(0.5) {
            Orientation::Horizontal
        } else {
            Orientation::Vertical
        };
        let origin = Coordinate {
            row: rng.random_range(0..=u8::MAX),
            col: rng.random_range(0..=u8::MAX),
        };
        let placement = ShipPlacement {
            kind,
            origin,
            orientation,
        };
        let ship_size = balance.ship_cells(kind).expect("registered ship kind");
        let should_fit = match orientation {
            Orientation::Horizontal => {
                origin.row < balance.board_size
                    && origin
                        .col
                        .checked_add(ship_size)
                        .is_some_and(|end| end <= balance.board_size)
            }
            Orientation::Vertical => {
                origin.col < balance.board_size
                    && origin
                        .row
                        .checked_add(ship_size)
                        .is_some_and(|end| end <= balance.board_size)
            }
        };

        match placement.cells_for(&balance) {
            Ok(cells) => {
                assert!(
                    should_fit,
                    "case {case}: an out-of-bounds placement was accepted"
                );
                assert_eq!(cells.len(), usize::from(ship_size), "case {case}");
                assert_eq!(
                    cells.iter().copied().collect::<HashSet<_>>().len(),
                    cells.len()
                );
                assert!(
                    cells
                        .iter()
                        .all(|cell| cell.row < balance.board_size && cell.col < balance.board_size)
                );
                for pair in cells.windows(2) {
                    let row_distance = pair[0].row.abs_diff(pair[1].row);
                    let col_distance = pair[0].col.abs_diff(pair[1].col);
                    assert_eq!(row_distance + col_distance, 1, "case {case}");
                }
            }
            Err(error) => {
                assert!(!should_fit, "case {case}: a valid placement was rejected");
                assert_eq!(error, GameError::PlacementOutOfBounds, "case {case}");
            }
        }
    }
}

#[test]
fn every_attack_permutation_preserves_hit_accounting_and_terminal_monotonicity() {
    let mut rng = StdRng::seed_from_u64(PROPERTY_SEED ^ 0x5555_aaaa);
    let all_coordinates: Vec<_> = (0..10)
        .flat_map(|row| (0..10).map(move |col| Coordinate { row, col }))
        .collect();

    for case in 0..256 {
        let mut coordinates = all_coordinates.clone();
        coordinates.shuffle(&mut rng);
        let mut board = Board::from_placements(&canonical_fleet()).expect("canonical fleet");
        let mut occupied_hits = 0;
        let mut sunk_ships = 0;
        let mut terminal_observed = false;

        for coordinate in coordinates {
            let result = board
                .attack(coordinate)
                .expect("first attack on a coordinate");
            if result.outcome != AttackOutcome::Miss {
                occupied_hits += 1;
            }
            if result.outcome == AttackOutcome::Sunk {
                sunk_ships += 1;
                assert!(result.sunk_ship.is_some(), "case {case}");
            } else {
                assert!(result.sunk_ship.is_none(), "case {case}");
            }
            if terminal_observed {
                assert!(result.all_sunk, "case {case}: terminal state regressed");
            }
            if result.all_sunk {
                terminal_observed = true;
                assert_eq!(occupied_hits, 17, "case {case}: premature victory");
            }
        }

        assert_eq!(occupied_hits, 17, "case {case}");
        assert_eq!(sunk_ships, 5, "case {case}");
        assert!(terminal_observed, "case {case}");
        assert_eq!(board.attacks_received().len(), 100, "case {case}");
    }
}

#[test]
fn coordinate_labels_are_unique_and_round_trip_the_entire_board() {
    let mut labels = HashSet::new();
    for row in 0..10 {
        for col in 0..10 {
            let coordinate = Coordinate::new(row, col).expect("board coordinate");
            let label = coordinate.label();
            assert_eq!(label.as_bytes()[0], b'A' + row);
            assert_eq!(&label[1..], (col + 1).to_string());
            assert!(labels.insert(label));
        }
    }
    assert_eq!(labels.len(), 100);
}
