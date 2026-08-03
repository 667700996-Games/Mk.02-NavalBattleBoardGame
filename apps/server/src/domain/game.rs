use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::GameError;

use super::{AttackOutcome, Board, Coordinate, ShipKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttackRecord {
    pub request_id: Uuid,
    pub attacker_id: Uuid,
    pub target_id: Uuid,
    pub coordinate: Coordinate,
    pub outcome: AttackOutcome,
    pub sunk_ship: Option<ShipKind>,
    pub turn_number: u32,
    pub next_player_id: Option<Uuid>,
    pub winner_id: Option<Uuid>,
    pub resolved_version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatistics {
    pub player_id: Uuid,
    pub shots: u32,
    pub hits: u32,
    pub ships_sunk: u8,
    pub accuracy: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameResult {
    pub winner_id: Uuid,
    pub loser_id: Uuid,
    pub total_turns: u32,
    pub duration_seconds: i64,
    pub finished_at: DateTime<Utc>,
    pub players: Vec<PlayerStatistics>,
    pub finish_reason: FinishReason,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinishReason {
    FleetDestroyed,
    DisconnectTimeout,
    PlayerLeft,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub boards: HashMap<Uuid, Board>,
    pub attacks: Vec<AttackRecord>,
    pub current_player_id: Uuid,
    pub turn_number: u32,
    pub started_at: DateTime<Utc>,
    pub result: Option<GameResult>,
}

impl Game {
    pub fn new(boards: HashMap<Uuid, Board>) -> Result<Self, GameError> {
        if boards.len() != 2 {
            return Err(GameError::InvalidState);
        }
        let player_ids: Vec<_> = boards.keys().copied().collect();
        let mut rng = rand::rng();
        let current_player_id = *player_ids.choose(&mut rng).ok_or(GameError::InvalidState)?;
        Ok(Self {
            boards,
            attacks: Vec::new(),
            current_player_id,
            turn_number: 1,
            started_at: Utc::now(),
            result: None,
        })
    }

    #[cfg(test)]
    pub fn new_with_first_player(
        boards: HashMap<Uuid, Board>,
        current_player_id: Uuid,
    ) -> Result<Self, GameError> {
        if boards.len() != 2 || !boards.contains_key(&current_player_id) {
            return Err(GameError::InvalidState);
        }
        Ok(Self {
            boards,
            attacks: Vec::new(),
            current_player_id,
            turn_number: 1,
            started_at: Utc::now(),
            result: None,
        })
    }

    pub fn previous_resolution(&self, request_id: Uuid, attacker_id: Uuid) -> Option<AttackRecord> {
        self.attacks
            .iter()
            .find(|attack| attack.request_id == request_id && attack.attacker_id == attacker_id)
            .cloned()
    }

    pub fn fire(
        &mut self,
        request_id: Uuid,
        attacker_id: Uuid,
        coordinate: Coordinate,
        expected_turn: u32,
        resolved_version: u64,
    ) -> Result<AttackRecord, GameError> {
        if self.result.is_some() {
            return Err(GameError::InvalidState);
        }
        if self.current_player_id != attacker_id {
            return Err(GameError::NotYourTurn);
        }
        if self.turn_number != expected_turn {
            return Err(GameError::TurnConflict);
        }

        let target_id = self
            .boards
            .keys()
            .copied()
            .find(|player_id| *player_id != attacker_id)
            .ok_or(GameError::InvalidState)?;
        let result = self
            .boards
            .get_mut(&target_id)
            .ok_or(GameError::InvalidState)?
            .attack(coordinate)?;

        let winner_id = result.all_sunk.then_some(attacker_id);
        let next_player_id = if winner_id.is_some() { None } else { Some(target_id) };
        let record = AttackRecord {
            request_id,
            attacker_id,
            target_id,
            coordinate,
            outcome: result.outcome,
            sunk_ship: result.sunk_ship,
            turn_number: self.turn_number,
            next_player_id,
            winner_id,
            resolved_version,
            created_at: Utc::now(),
        };
        self.attacks.push(record.clone());

        if winner_id.is_some() {
            self.finish(attacker_id, target_id, FinishReason::FleetDestroyed);
        } else {
            self.current_player_id = target_id;
            self.turn_number += 1;
        }
        Ok(record)
    }

    pub fn forfeit(&mut self, winner_id: Uuid, reason: FinishReason) -> Result<(), GameError> {
        let loser_id = self
            .boards
            .keys()
            .copied()
            .find(|player_id| *player_id != winner_id)
            .ok_or(GameError::InvalidState)?;
        self.finish(winner_id, loser_id, reason);
        Ok(())
    }

    fn finish(&mut self, winner_id: Uuid, loser_id: Uuid, reason: FinishReason) {
        let finished_at = Utc::now();
        let players = self
            .boards
            .keys()
            .map(|player_id| {
                let attacks: Vec<_> = self
                    .attacks
                    .iter()
                    .filter(|attack| attack.attacker_id == *player_id)
                    .collect();
                let hits = attacks
                    .iter()
                    .filter(|attack| attack.outcome != AttackOutcome::Miss)
                    .count() as u32;
                let shots = attacks.len() as u32;
                PlayerStatistics {
                    player_id: *player_id,
                    shots,
                    hits,
                    ships_sunk: attacks.iter().filter(|attack| attack.sunk_ship.is_some()).count() as u8,
                    accuracy: if shots == 0 { 0.0 } else { hits as f32 / shots as f32 },
                }
            })
            .collect();
        self.result = Some(GameResult {
            winner_id,
            loser_id,
            total_turns: self.attacks.len() as u32,
            duration_seconds: (finished_at - self.started_at).num_seconds().max(0),
            finished_at,
            players,
            finish_reason: reason,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Orientation, ShipKind, ShipPlacement};

    fn board_at(row_offset: u8) -> Board {
        Board::from_placements(&[
            ShipPlacement { kind: ShipKind::Carrier, origin: Coordinate { row: row_offset, col: 0 }, orientation: Orientation::Horizontal },
            ShipPlacement { kind: ShipKind::Battleship, origin: Coordinate { row: row_offset + 1, col: 0 }, orientation: Orientation::Horizontal },
            ShipPlacement { kind: ShipKind::Cruiser, origin: Coordinate { row: row_offset + 2, col: 0 }, orientation: Orientation::Horizontal },
            ShipPlacement { kind: ShipKind::Submarine, origin: Coordinate { row: row_offset + 3, col: 0 }, orientation: Orientation::Horizontal },
            ShipPlacement { kind: ShipKind::Destroyer, origin: Coordinate { row: row_offset + 4, col: 0 }, orientation: Orientation::Horizontal },
        ]).unwrap()
    }

    #[test]
    fn enforces_turn_and_switches_after_one_shot() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut game = Game::new_with_first_player(HashMap::from([(first, board_at(0)), (second, board_at(5))]), first).unwrap();
        assert_eq!(game.fire(Uuid::new_v4(), second, Coordinate { row: 9, col: 9 }, 1, 1).unwrap_err(), GameError::NotYourTurn);
        game.fire(Uuid::new_v4(), first, Coordinate { row: 0, col: 9 }, 1, 1).unwrap();
        assert_eq!(game.current_player_id, second);
        assert_eq!(game.turn_number, 2);
    }
}

