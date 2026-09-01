use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::GameError;

use super::{
    AttackOutcome, BalanceManifest, BalancePin, Board, Coordinate, ShipKind, TacticalSkillGrade,
    TacticalSkillKind, TacticalSkillRules,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GameMode {
    #[default]
    Classic,
    Rapid,
    Salvo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MatchRules {
    #[serde(default)]
    pub mode: GameMode,
    #[serde(default)]
    pub turn_duration_seconds: Option<u32>,
    #[serde(default)]
    pub tactical_skills_enabled: bool,
}

impl MatchRules {
    pub fn validate(self) -> Result<Self, GameError> {
        self.validate_for(&BalancePin::current().manifest)
    }

    pub fn validate_for(self, balance: &BalanceManifest) -> Result<Self, GameError> {
        if self.tactical_skills_enabled && balance.tactical_skills.is_none() {
            return Err(GameError::InvalidRequest);
        }
        if self
            .turn_duration_seconds
            .is_some_and(|seconds| seconds > balance.maximum_turn_duration_seconds)
        {
            return Err(GameError::InvalidRequest);
        }
        Ok(self)
    }

    pub fn resolved_turn_duration(self, fallback: u32) -> u32 {
        self.resolved_turn_duration_for(fallback, &BalancePin::current().manifest)
    }

    pub fn resolved_turn_duration_for(self, fallback: u32, balance: &BalanceManifest) -> u32 {
        match self.mode {
            GameMode::Rapid => balance.rapid_turn_duration_seconds,
            GameMode::Classic | GameMode::Salvo => self
                .turn_duration_seconds
                .unwrap_or(fallback)
                .min(balance.maximum_turn_duration_seconds),
        }
    }
}

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
    #[serde(default = "default_one_shot")]
    pub shots_remaining_in_turn: u8,
    pub resolved_version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalSkillInventory {
    pub rapid_fire: u8,
    pub cross_fire: u8,
    pub area_annihilation: u8,
}

impl TacticalSkillInventory {
    pub fn from_rules(rules: &TacticalSkillRules) -> Self {
        let mut inventory = Self::default();
        for skill in &rules.skills {
            *inventory.remaining_mut(skill.kind) = skill.uses_per_match;
        }
        inventory
    }

    pub fn remaining(self, kind: TacticalSkillKind) -> u8 {
        match kind {
            TacticalSkillKind::RapidFire => self.rapid_fire,
            TacticalSkillKind::CrossFire => self.cross_fire,
            TacticalSkillKind::AreaAnnihilation => self.area_annihilation,
        }
    }

    fn remaining_mut(&mut self, kind: TacticalSkillKind) -> &mut u8 {
        match kind {
            TacticalSkillKind::RapidFire => &mut self.rapid_fire,
            TacticalSkillKind::CrossFire => &mut self.cross_fire,
            TacticalSkillKind::AreaAnnihilation => &mut self.area_annihilation,
        }
    }

    fn consume(&mut self, kind: TacticalSkillKind) -> Result<u8, GameError> {
        let remaining = self.remaining_mut(kind);
        if *remaining == 0 {
            return Err(GameError::TacticalSkillExhausted);
        }
        *remaining -= 1;
        Ok(*remaining)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalSkillCellResult {
    pub coordinate: Coordinate,
    pub outcome: AttackOutcome,
    pub sunk_ship: Option<ShipKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalSkillUseRecord {
    pub request_id: Uuid,
    pub attacker_id: Uuid,
    pub target_id: Uuid,
    pub skill: TacticalSkillKind,
    pub grade: TacticalSkillGrade,
    pub cells: Vec<TacticalSkillCellResult>,
    pub turn_number: u32,
    pub next_player_id: Option<Uuid>,
    pub winner_id: Option<Uuid>,
    pub shots_remaining_in_turn: u8,
    pub remaining_uses: u8,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(tag = "type", content = "payload", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GameTimelineEvent {
    Attack(AttackRecord),
    SkillAttack(TacticalSkillUseRecord),
    TurnExpired(TurnExpiration),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    #[serde(default)]
    pub balance: BalancePin,
    pub boards: HashMap<Uuid, Board>,
    pub attacks: Vec<AttackRecord>,
    #[serde(default)]
    pub skill_uses: Vec<TacticalSkillUseRecord>,
    #[serde(default)]
    pub timeline: Vec<GameTimelineEvent>,
    #[serde(default)]
    pub first_player_id: Uuid,
    #[serde(default)]
    pub mode: GameMode,
    #[serde(default)]
    pub tactical_skills_enabled: bool,
    #[serde(default)]
    pub skill_inventories: HashMap<Uuid, TacticalSkillInventory>,
    #[serde(default)]
    pub skill_used_turns: HashMap<Uuid, u32>,
    #[serde(default = "default_one_shot")]
    pub shots_remaining_in_turn: u8,
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
        Self::new_with_rules(
            boards,
            MatchRules {
                mode: GameMode::Classic,
                turn_duration_seconds: Some(turn_duration_seconds),
                tactical_skills_enabled: false,
            },
            turn_duration_seconds,
        )
    }

    pub fn new_with_rules(
        boards: HashMap<Uuid, Board>,
        rules: MatchRules,
        fallback_turn_duration_seconds: u32,
    ) -> Result<Self, GameError> {
        Self::new_with_rules_and_balance(
            boards,
            rules,
            fallback_turn_duration_seconds,
            BalancePin::current(),
        )
    }

    pub fn new_with_rules_and_balance(
        boards: HashMap<Uuid, Board>,
        rules: MatchRules,
        fallback_turn_duration_seconds: u32,
        balance: BalancePin,
    ) -> Result<Self, GameError> {
        if boards.len() != 2 {
            return Err(GameError::InvalidState);
        }
        if !balance.is_registered_for_execution() {
            return Err(GameError::InvalidState);
        }
        let rules = rules.validate_for(&balance.manifest)?;
        let player_ids: Vec<_> = boards.keys().copied().collect();
        let mut rng = rand::rng();
        let current_player_id = *player_ids.choose(&mut rng).ok_or(GameError::InvalidState)?;
        let shots_remaining_in_turn =
            shots_for_mode(&boards, current_player_id, rules.mode, &balance.manifest);
        let turn_duration_seconds =
            rules.resolved_turn_duration_for(fallback_turn_duration_seconds, &balance.manifest);
        let now = Utc::now();
        let skill_inventories = if rules.tactical_skills_enabled {
            let skill_rules = balance
                .manifest
                .tactical_skills
                .as_ref()
                .ok_or(GameError::InvalidState)?;
            player_ids
                .iter()
                .map(|player_id| (*player_id, TacticalSkillInventory::from_rules(skill_rules)))
                .collect()
        } else {
            HashMap::new()
        };
        Ok(Self {
            balance,
            boards,
            attacks: Vec::new(),
            skill_uses: Vec::new(),
            timeline: Vec::new(),
            first_player_id: current_player_id,
            mode: rules.mode,
            tactical_skills_enabled: rules.tactical_skills_enabled,
            skill_inventories,
            skill_used_turns: HashMap::new(),
            shots_remaining_in_turn,
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
            balance: BalancePin::current(),
            boards,
            attacks: Vec::new(),
            skill_uses: Vec::new(),
            timeline: Vec::new(),
            first_player_id: current_player_id,
            mode: GameMode::Classic,
            tactical_skills_enabled: false,
            skill_inventories: HashMap::new(),
            skill_used_turns: HashMap::new(),
            shots_remaining_in_turn: 1,
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

    pub fn previous_skill_resolution(
        &self,
        request_id: Uuid,
        attacker_id: Uuid,
    ) -> Option<TacticalSkillUseRecord> {
        self.skill_uses
            .iter()
            .find(|record| record.request_id == request_id && record.attacker_id == attacker_id)
            .cloned()
    }

    pub fn skill_inventory(&self, player_id: Uuid) -> TacticalSkillInventory {
        self.skill_inventories
            .get(&player_id)
            .copied()
            .unwrap_or_default()
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
        if !self.balance.is_registered_for_execution() {
            return Err(GameError::InvalidState);
        }
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
        let continues_salvo =
            winner_id.is_none() && self.mode == GameMode::Salvo && self.shots_remaining_in_turn > 1;
        let next_player_id = if winner_id.is_some() {
            None
        } else if continues_salvo {
            Some(attacker_id)
        } else {
            Some(target_id)
        };
        let shots_remaining_in_turn = if winner_id.is_some() {
            0
        } else if continues_salvo {
            self.shots_remaining_in_turn.saturating_sub(1)
        } else {
            shots_for_mode(&self.boards, target_id, self.mode, &self.balance.manifest)
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
            shots_remaining_in_turn,
            resolved_version,
            created_at: now,
        };
        self.attacks.push(record.clone());
        self.timeline
            .push(GameTimelineEvent::Attack(record.clone()));
        self.consecutive_timeout_counts.insert(attacker_id, 0);

        if winner_id.is_some() {
            self.finish_at(attacker_id, target_id, FinishReason::FleetDestroyed, now);
            self.shots_remaining_in_turn = 0;
        } else if continues_salvo {
            self.shots_remaining_in_turn = self.shots_remaining_in_turn.saturating_sub(1);
        } else {
            self.current_player_id = target_id;
            self.turn_number += 1;
            self.start_turn_at(now);
        }
        Ok(record)
    }

    pub fn fire_skill(
        &mut self,
        request_id: Uuid,
        attacker_id: Uuid,
        skill: TacticalSkillKind,
        targets: Vec<Coordinate>,
        expected_turn: u32,
        resolved_version: u64,
    ) -> Result<TacticalSkillUseRecord, GameError> {
        self.fire_skill_at(
            request_id,
            attacker_id,
            skill,
            targets,
            expected_turn,
            resolved_version,
            Utc::now(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fire_skill_at(
        &mut self,
        request_id: Uuid,
        attacker_id: Uuid,
        skill: TacticalSkillKind,
        targets: Vec<Coordinate>,
        expected_turn: u32,
        resolved_version: u64,
        now: DateTime<Utc>,
    ) -> Result<TacticalSkillUseRecord, GameError> {
        if !self.balance.is_registered_for_execution() {
            return Err(GameError::InvalidState);
        }
        if !self.tactical_skills_enabled {
            return Err(GameError::TacticalSkillsDisabled);
        }
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
        let skill_rules = self
            .balance
            .manifest
            .tactical_skills
            .as_ref()
            .ok_or(GameError::InvalidState)?;
        if self.turn_number < skill_rules.unlock_turn {
            return Err(GameError::TacticalSkillLocked);
        }
        if self.skill_used_turns.get(&attacker_id) == Some(&self.turn_number) {
            return Err(GameError::TacticalSkillAlreadyUsed);
        }
        let spec = skill_rules.spec(skill).ok_or(GameError::InvalidRequest)?;
        if self.skill_inventory(attacker_id).remaining(skill) == 0 {
            return Err(GameError::TacticalSkillExhausted);
        }
        let target_id = self
            .boards
            .keys()
            .copied()
            .find(|player_id| *player_id != attacker_id)
            .ok_or(GameError::InvalidState)?;
        let board = self.boards.get(&target_id).ok_or(GameError::InvalidState)?;
        let mut coordinates = skill_coordinates(skill, &targets, self.balance.manifest.board_size)?;
        if skill == TacticalSkillKind::RapidFire
            && coordinates
                .iter()
                .any(|coordinate| board.was_attacked(*coordinate))
        {
            return Err(GameError::InvalidTacticalSkillTargets);
        }
        coordinates.retain(|coordinate| !board.was_attacked(*coordinate));

        let board = self
            .boards
            .get_mut(&target_id)
            .ok_or(GameError::InvalidState)?;
        let mut cells = Vec::with_capacity(coordinates.len());
        for coordinate in coordinates {
            let result = board.attack(coordinate)?;
            cells.push(TacticalSkillCellResult {
                coordinate,
                outcome: result.outcome,
                sunk_ship: result.sunk_ship,
            });
        }
        let winner_id = board
            .ships()
            .iter()
            .all(|ship| ship.is_sunk())
            .then_some(attacker_id);
        let remaining_uses = self
            .skill_inventories
            .get_mut(&attacker_id)
            .ok_or(GameError::InvalidState)?
            .consume(skill)?;
        self.skill_used_turns.insert(attacker_id, self.turn_number);

        let continues_salvo =
            winner_id.is_none() && self.mode == GameMode::Salvo && self.shots_remaining_in_turn > 1;
        let next_player_id = if winner_id.is_some() {
            None
        } else if continues_salvo {
            Some(attacker_id)
        } else {
            Some(target_id)
        };
        let shots_remaining_in_turn = if winner_id.is_some() {
            0
        } else if continues_salvo {
            self.shots_remaining_in_turn.saturating_sub(1)
        } else {
            shots_for_mode(&self.boards, target_id, self.mode, &self.balance.manifest)
        };
        let record = TacticalSkillUseRecord {
            request_id,
            attacker_id,
            target_id,
            skill,
            grade: spec.grade,
            cells,
            turn_number: self.turn_number,
            next_player_id,
            winner_id,
            shots_remaining_in_turn,
            remaining_uses,
            resolved_version,
            created_at: now,
        };
        self.skill_uses.push(record.clone());
        self.timeline
            .push(GameTimelineEvent::SkillAttack(record.clone()));
        self.consecutive_timeout_counts.insert(attacker_id, 0);

        if winner_id.is_some() {
            self.finish_at(attacker_id, target_id, FinishReason::FleetDestroyed, now);
            self.shots_remaining_in_turn = 0;
        } else if continues_salvo {
            self.shots_remaining_in_turn = self.shots_remaining_in_turn.saturating_sub(1);
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
        if !self.balance.is_registered_for_execution() {
            return Err(GameError::InvalidState);
        }
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
        let winner_id =
            if consecutive_timeout_count >= self.balance.manifest.consecutive_timeout_forfeit {
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
        let expiration = TurnExpiration {
            expired_turn_number: expected_turn,
            expired_player_id: expected_player_id,
            next_player_id: winner_id.is_none().then_some(next_player_id),
            consecutive_timeout_count,
            total_timeout_count,
            winner_id,
            expired_at: now,
        };
        self.timeline
            .push(GameTimelineEvent::TurnExpired(expiration.clone()));
        Ok(Some(expiration))
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
        self.shots_remaining_in_turn = shots_for_mode(
            &self.boards,
            self.current_player_id,
            self.mode,
            &self.balance.manifest,
        );
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
                let skill_cells: Vec<_> = self
                    .skill_uses
                    .iter()
                    .filter(|record| record.attacker_id == *player_id)
                    .flat_map(|record| record.cells.iter())
                    .collect();
                let hits = attacks
                    .iter()
                    .filter(|attack| attack.outcome != AttackOutcome::Miss)
                    .count() as u32
                    + skill_cells
                        .iter()
                        .filter(|cell| cell.outcome != AttackOutcome::Miss)
                        .count() as u32;
                let shots = (attacks.len() + skill_cells.len()) as u32;
                PlayerStatistics {
                    player_id: *player_id,
                    shots,
                    hits,
                    ships_sunk: attacks
                        .iter()
                        .filter(|attack| attack.sunk_ship.is_some())
                        .count() as u8
                        + skill_cells
                            .iter()
                            .filter(|cell| cell.sunk_ship.is_some())
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

fn default_one_shot() -> u8 {
    1
}

fn skill_coordinates(
    skill: TacticalSkillKind,
    targets: &[Coordinate],
    board_size: u8,
) -> Result<Vec<Coordinate>, GameError> {
    let validate = |coordinate: Coordinate| {
        Coordinate::new_for_board(coordinate.row, coordinate.col, board_size)
            .map_err(|_| GameError::InvalidTacticalSkillTargets)
    };
    let mut coordinates = match skill {
        TacticalSkillKind::RapidFire => {
            if targets.len() != 2 || targets[0] == targets[1] {
                return Err(GameError::InvalidTacticalSkillTargets);
            }
            vec![validate(targets[0])?, validate(targets[1])?]
        }
        TacticalSkillKind::CrossFire => {
            if targets.len() != 1 {
                return Err(GameError::InvalidTacticalSkillTargets);
            }
            let center = validate(targets[0])?;
            coordinates_for_offsets(
                center,
                board_size,
                &[(0, 0), (-1, 0), (0, -1), (0, 1), (1, 0)],
            )
        }
        TacticalSkillKind::AreaAnnihilation => {
            if targets.len() != 1 {
                return Err(GameError::InvalidTacticalSkillTargets);
            }
            let center = validate(targets[0])?;
            let offsets = [
                (-1, -1),
                (-1, 0),
                (-1, 1),
                (0, -1),
                (0, 0),
                (0, 1),
                (1, -1),
                (1, 0),
                (1, 1),
            ];
            coordinates_for_offsets(center, board_size, &offsets)
        }
    };
    coordinates.sort_by_key(|coordinate| (coordinate.row, coordinate.col));
    coordinates.dedup();
    Ok(coordinates)
}

fn coordinates_for_offsets(
    center: Coordinate,
    board_size: u8,
    offsets: &[(i16, i16)],
) -> Vec<Coordinate> {
    offsets
        .iter()
        .filter_map(|(row_offset, col_offset)| {
            let row = i16::from(center.row) + row_offset;
            let col = i16::from(center.col) + col_offset;
            (row >= 0 && col >= 0 && row < i16::from(board_size) && col < i16::from(board_size))
                .then_some(Coordinate {
                    row: row as u8,
                    col: col as u8,
                })
        })
        .collect()
}

fn shots_for_mode(
    boards: &HashMap<Uuid, Board>,
    player_id: Uuid,
    mode: GameMode,
    balance: &BalanceManifest,
) -> u8 {
    if mode != GameMode::Salvo {
        return balance.classic_shots_per_turn;
    }
    boards
        .get(&player_id)
        .map(|board| board.ships().iter().filter(|ship| !ship.is_sunk()).count() as u8)
        .unwrap_or(1)
        .max(1)
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

    #[test]
    fn salvo_keeps_authority_for_each_surviving_ship_then_changes_turn() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut game = Game::new_with_first_player(
            HashMap::from([(first, board_at(0)), (second, board_at(5))]),
            first,
        )
        .unwrap();
        game.mode = GameMode::Salvo;
        game.shots_remaining_in_turn = 5;

        for shot in 0_u8..5 {
            let record = game
                .fire(
                    Uuid::new_v4(),
                    first,
                    Coordinate { row: shot, col: 9 },
                    1,
                    u64::from(shot) + 1,
                )
                .unwrap();
            if shot < 4 {
                assert_eq!(game.current_player_id, first);
                assert_eq!(game.turn_number, 1);
                assert_eq!(record.next_player_id, Some(first));
                assert_eq!(record.shots_remaining_in_turn, 4 - shot);
            }
        }
        assert_eq!(game.current_player_id, second);
        assert_eq!(game.turn_number, 2);
        assert_eq!(game.shots_remaining_in_turn, 5);
    }

    fn tactical_game(first: Uuid, second: Uuid, mode: GameMode) -> Game {
        let mut game = Game::new_with_rules_and_balance(
            HashMap::from([(first, board_at(0)), (second, board_at(5))]),
            MatchRules {
                mode,
                turn_duration_seconds: Some(60),
                tactical_skills_enabled: true,
            },
            60,
            BalancePin::v2(),
        )
        .unwrap();
        game.first_player_id = first;
        game.current_player_id = first;
        game.turn_number = 3;
        game.shots_remaining_in_turn = if mode == GameMode::Salvo { 5 } else { 1 };
        game
    }

    #[test]
    fn tactical_cross_fire_is_server_generated_and_consumes_one_classic_turn() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut game = tactical_game(first, second, GameMode::Classic);
        let request_id = Uuid::new_v4();
        let record = game
            .fire_skill(
                request_id,
                first,
                TacticalSkillKind::CrossFire,
                vec![Coordinate { row: 0, col: 0 }],
                3,
                2,
            )
            .unwrap();
        assert_eq!(
            record
                .cells
                .iter()
                .map(|cell| cell.coordinate)
                .collect::<Vec<_>>(),
            vec![
                Coordinate { row: 0, col: 0 },
                Coordinate { row: 0, col: 1 },
                Coordinate { row: 1, col: 0 },
            ]
        );
        assert_eq!(record.remaining_uses, 1);
        assert_eq!(record.next_player_id, Some(second));
        assert_eq!(game.current_player_id, second);
        assert_eq!(game.turn_number, 4);
        assert_eq!(
            game.previous_skill_resolution(request_id, first),
            Some(record)
        );
    }

    #[test]
    fn tactical_salvo_uses_one_shell_and_rejects_a_second_skill_in_the_same_turn() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut game = tactical_game(first, second, GameMode::Salvo);
        let record = game
            .fire_skill(
                Uuid::new_v4(),
                first,
                TacticalSkillKind::RapidFire,
                vec![
                    Coordinate { row: 8, col: 8 },
                    Coordinate { row: 9, col: 9 },
                ],
                3,
                2,
            )
            .unwrap();
        assert_eq!(record.cells.len(), 2);
        assert_eq!(record.remaining_uses, 2);
        assert_eq!(record.shots_remaining_in_turn, 4);
        assert_eq!(game.current_player_id, first);
        assert_eq!(game.turn_number, 3);
        assert_eq!(
            game.fire_skill(
                Uuid::new_v4(),
                first,
                TacticalSkillKind::AreaAnnihilation,
                vec![Coordinate { row: 4, col: 4 }],
                3,
                3,
            )
            .unwrap_err(),
            GameError::TacticalSkillAlreadyUsed
        );
    }
}
