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
    #[serde(default)]
    pub total_timeouts: u32,
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
    #[serde(default)]
    pub win_type: WinType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinishReason {
    FleetDestroyed,
    Surrender,
    TurnTimeout,
    DisconnectTimeout,
    PlayerLeft,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WinType {
    #[default]
    NormalVictory,
    Surrender,
    Disconnect,
    Timeout,
}

impl From<FinishReason> for WinType {
    fn from(reason: FinishReason) -> Self {
        match reason {
            FinishReason::FleetDestroyed => Self::NormalVictory,
            FinishReason::Surrender => Self::Surrender,
            FinishReason::TurnTimeout => Self::Timeout,
            FinishReason::DisconnectTimeout | FinishReason::PlayerLeft => Self::Disconnect,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnExpiration {
    pub expired_turn_number: u32,
    pub expired_player_id: Uuid,
    pub next_player_id: Option<Uuid>,
    pub consecutive_timeout_count: u8,
    pub total_timeout_count: u32,
    pub winner_id: Option<Uuid>,
    pub expired_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub boards: HashMap<Uuid, Board>,
    pub attacks: Vec<AttackRecord>,
    pub current_player_id: Uuid,
    pub turn_number: u32,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub turn_duration_seconds: u32,
    #[serde(default)]
    pub turn_started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub turn_deadline_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub consecutive_timeout_counts: HashMap<Uuid, u8>,
    #[serde(default)]
    pub total_timeout_counts: HashMap<Uuid, u32>,
    pub result: Option<GameResult>,
}

impl Game {
    pub fn new(boards: HashMap<Uuid, Board>) -> Result<Self, GameError> {
        Self::new_with_turn_duration(boards, 60)
    }

    pub fn new_with_turn_duration(
        boards: HashMap<Uuid, Board>,
        turn_duration_seconds: u32,
    ) -> Result<Self, GameError> {
        if boards.len() != 2 {
            return Err(GameError::InvalidState);
        }
        let player_ids: Vec<_> = boards.keys().copied().collect();
        let mut rng = rand::rng();
        let current_player_id = *player_ids.choose(&mut rng).ok_or(GameError::InvalidState)?;
        let now = Utc::now();
        Ok(Self {
            boards,
            attacks: Vec::new(),
            current_player_id,
            turn_number: 1,
            started_at: now,
            turn_duration_seconds,
            turn_started_at: Some(now),
            turn_deadline_at: deadline_from(now, turn_duration_seconds),
            consecutive_timeout_counts: HashMap::new(),
            total_timeout_counts: HashMap::new(),
            result: None,
        })
    }

    #[cfg(test)]
    pub fn new_with_first_player(
        boards: HashMap<Uuid, Board>,
        current_player_id: Uuid,
    ) -> Result<Self, GameError> {
        Self::new_with_first_player_and_duration(boards, current_player_id, 60)
    }

    #[cfg(test)]
    pub fn new_with_first_player_and_duration(
        boards: HashMap<Uuid, Board>,
        current_player_id: Uuid,
        turn_duration_seconds: u32,
    ) -> Result<Self, GameError> {
        if boards.len() != 2 || !boards.contains_key(&current_player_id) {
            return Err(GameError::InvalidState);
        }
        let now = Utc::now();
        Ok(Self {
            boards,
            attacks: Vec::new(),
            current_player_id,
            turn_number: 1,
            started_at: now,
            turn_duration_seconds,
            turn_started_at: Some(now),
            turn_deadline_at: deadline_from(now, turn_duration_seconds),
            consecutive_timeout_counts: HashMap::new(),
            total_timeout_counts: HashMap::new(),
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
        self.fire_at(
            request_id,
            attacker_id,
            coordinate,
            expected_turn,
            resolved_version,
            Utc::now(),
        )
    }

    pub fn fire_at(
        &mut self,
        request_id: Uuid,
        attacker_id: Uuid,
        coordinate: Coordinate,
        expected_turn: u32,
        resolved_version: u64,
        now: DateTime<Utc>,
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
        if self
            .turn_deadline_at
            .is_some_and(|deadline| now >= deadline)
        {
            return Err(GameError::TurnExpired);
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
        let next_player_id = if winner_id.is_some() {
            None
        } else {
            Some(target_id)
        };
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
            created_at: now,
        };
        self.attacks.push(record.clone());
        self.consecutive_timeout_counts.insert(attacker_id, 0);

        if winner_id.is_some() {
            self.finish_at(attacker_id, target_id, FinishReason::FleetDestroyed, now);
        } else {
            self.current_player_id = target_id;
            self.turn_number += 1;
            self.start_turn_at(now);
        }
        Ok(record)
    }

    pub fn ensure_turn_timer(&mut self, turn_duration_seconds: u32, now: DateTime<Utc>) -> bool {
        if self.result.is_some() {
            return false;
        }
        let mut changed = false;
        if self.turn_duration_seconds == 0 && turn_duration_seconds > 0 {
            self.turn_duration_seconds = turn_duration_seconds;
            changed = true;
        }
        if self.turn_started_at.is_none() {
            self.turn_started_at = Some(now);
            changed = true;
        }
        if self.turn_deadline_at.is_none() && self.turn_duration_seconds > 0 {
            self.turn_deadline_at = deadline_from(now, self.turn_duration_seconds);
            changed = true;
        }
        changed
    }

    pub fn expire_turn(
        &mut self,
        expected_turn: u32,
        expected_player_id: Uuid,
        expected_deadline: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Option<TurnExpiration>, GameError> {
        if self.result.is_some()
            || self.turn_number != expected_turn
            || self.current_player_id != expected_player_id
            || self.turn_deadline_at != Some(expected_deadline)
            || now < expected_deadline
        {
            return Ok(None);
        }
        let next_player_id = self
            .boards
            .keys()
            .copied()
            .find(|player_id| *player_id != expected_player_id)
            .ok_or(GameError::InvalidState)?;
        let consecutive = self
            .consecutive_timeout_counts
            .entry(expected_player_id)
            .or_default();
        *consecutive = consecutive.saturating_add(1);
        let consecutive_timeout_count = *consecutive;
        let total = self
            .total_timeout_counts
            .entry(expected_player_id)
            .or_default();
        *total = total.saturating_add(1);
        let total_timeout_count = *total;
        let winner_id = if consecutive_timeout_count >= 3 {
            self.finish_at(
                next_player_id,
                expected_player_id,
                FinishReason::TurnTimeout,
                now,
            );
            Some(next_player_id)
        } else {
            self.current_player_id = next_player_id;
            self.turn_number += 1;
            self.start_turn_at(now);
            None
        };
        Ok(Some(TurnExpiration {
            expired_turn_number: expected_turn,
            expired_player_id: expected_player_id,
            next_player_id: winner_id.is_none().then_some(next_player_id),
            consecutive_timeout_count,
            total_timeout_count,
            winner_id,
            expired_at: now,
        }))
    }

    pub fn forfeit(&mut self, winner_id: Uuid, reason: FinishReason) -> Result<(), GameError> {
        if self.result.is_some() || !self.boards.contains_key(&winner_id) {
            return Err(GameError::InvalidState);
        }
        let loser_id = self
            .boards
            .keys()
            .copied()
            .find(|player_id| *player_id != winner_id)
            .ok_or(GameError::InvalidState)?;
        self.finish_at(winner_id, loser_id, reason, Utc::now());
        Ok(())
    }

    fn start_turn_at(&mut self, now: DateTime<Utc>) {
        self.turn_started_at = Some(now);
        self.turn_deadline_at = deadline_from(now, self.turn_duration_seconds);
    }

    fn finish_at(
        &mut self,
        winner_id: Uuid,
        loser_id: Uuid,
        reason: FinishReason,
        finished_at: DateTime<Utc>,
    ) {
        self.turn_deadline_at = None;
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
                    ships_sunk: attacks
                        .iter()
                        .filter(|attack| attack.sunk_ship.is_some())
                        .count() as u8,
                    accuracy: if shots == 0 {
                        0.0
                    } else {
                        hits as f32 / shots as f32
                    },
                    total_timeouts: self
                        .total_timeout_counts
                        .get(player_id)
                        .copied()
                        .unwrap_or_default(),
                }
            })
            .collect();
        self.result = Some(GameResult {
            winner_id,
            loser_id,
            total_turns: self.turn_number,
            duration_seconds: (finished_at - self.started_at).num_seconds().max(0),
            finished_at,
            players,
            finish_reason: reason,
            win_type: reason.into(),
        });
    }
}

fn deadline_from(now: DateTime<Utc>, duration_seconds: u32) -> Option<DateTime<Utc>> {
    (duration_seconds > 0).then(|| now + chrono::Duration::seconds(i64::from(duration_seconds)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Orientation, ShipKind, ShipPlacement};

    fn board_at(row_offset: u8) -> Board {
        Board::from_placements(&[
            ShipPlacement {
                kind: ShipKind::Carrier,
                origin: Coordinate {
                    row: row_offset,
                    col: 0,
                },
                orientation: Orientation::Horizontal,
            },
            ShipPlacement {
                kind: ShipKind::Battleship,
                origin: Coordinate {
                    row: row_offset + 1,
                    col: 0,
                },
                orientation: Orientation::Horizontal,
            },
            ShipPlacement {
                kind: ShipKind::Cruiser,
                origin: Coordinate {
                    row: row_offset + 2,
                    col: 0,
                },
                orientation: Orientation::Horizontal,
            },
            ShipPlacement {
                kind: ShipKind::Submarine,
                origin: Coordinate {
                    row: row_offset + 3,
                    col: 0,
                },
                orientation: Orientation::Horizontal,
            },
            ShipPlacement {
                kind: ShipKind::Destroyer,
                origin: Coordinate {
                    row: row_offset + 4,
                    col: 0,
                },
                orientation: Orientation::Horizontal,
            },
        ])
        .unwrap()
    }

    #[test]
    fn enforces_turn_and_switches_after_one_shot() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut game = Game::new_with_first_player(
            HashMap::from([(first, board_at(0)), (second, board_at(5))]),
            first,
        )
        .unwrap();
        assert_eq!(
            game.fire(Uuid::new_v4(), second, Coordinate { row: 9, col: 9 }, 1, 1)
                .unwrap_err(),
            GameError::NotYourTurn
        );
        game.fire(Uuid::new_v4(), first, Coordinate { row: 0, col: 9 }, 1, 1)
            .unwrap();
        assert_eq!(game.current_player_id, second);
        assert_eq!(game.turn_number, 2);
    }
}
