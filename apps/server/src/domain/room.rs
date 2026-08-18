use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::GameError;

use super::{
    ALLOWED_EMOJIS, AttackOutcome, AttackRecord, BalancePin, Board, ChatMessage, ChatMessageType,
    ChatTypingEvent, ConnectionState, Coordinate, FinishReason, Game, GameResult,
    GameTimelineEvent, MAX_CHAT_HISTORY, MAX_CHAT_MESSAGE_CHARS, MatchRules, MatchmakingQuality,
    Player, PlayerKind, PlayerReadyState, PlayerRole, QuickCommandId, RankedMatchContext, ShipKind,
    ShipPlacement, SurrenderRecord, TurnExpiration, UserSession,
};

mod chat;
mod projection;
#[cfg(test)]
mod spectator_tests;
mod state;
mod timers;

pub use projection::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerReadyRecord {
    pub request_id: Uuid,
    pub room_id: Uuid,
    pub player_id: Uuid,
    pub ready_state: PlayerReadyState,
    pub room_state: RoomStatus,
    pub version: u64,
    pub accepted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameStartRecord {
    pub request_id: Uuid,
    pub room_id: Uuid,
    pub game_id: Uuid,
    pub started_by: Uuid,
    pub version: u64,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameTimerState {
    pub room_id: Uuid,
    pub game_id: Uuid,
    pub turn_number: u32,
    pub active_player_id: Uuid,
    pub game_started_at: DateTime<Utc>,
    pub turn_started_at: Option<DateTime<Utc>>,
    pub turn_deadline_at: Option<DateTime<Utc>>,
    pub turn_duration_seconds: u32,
    pub server_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnExpiredRecord {
    pub room_id: Uuid,
    pub game_id: Uuid,
    pub expired_turn_number: u32,
    pub expired_player_id: Uuid,
    pub next_player_id: Option<Uuid>,
    pub consecutive_timeout_count: u8,
    pub total_timeout_count: u32,
    pub winner_id: Option<Uuid>,
    pub expired_at: DateTime<Utc>,
    pub server_timestamp: DateTime<Utc>,
}

impl TurnExpiredRecord {
    fn from_expiration(
        room_id: Uuid,
        game_id: Uuid,
        expiration: TurnExpiration,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            room_id,
            game_id,
            expired_turn_number: expiration.expired_turn_number,
            expired_player_id: expiration.expired_player_id,
            next_player_id: expiration.next_player_id,
            consecutive_timeout_count: expiration.consecutive_timeout_count,
            total_timeout_count: expiration.total_timeout_count,
            winner_id: expiration.winner_id,
            expired_at: expiration.expired_at,
            server_timestamp: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoomStatus {
    #[serde(alias = "WAITING")]
    WaitingForOpponent,
    WaitingForReady,
    #[serde(alias = "READY")]
    ReadyToStart,
    #[serde(alias = "DISCONNECTED")]
    Placement,
    Playing,
    Finished,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoomVisibility {
    Public,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AiDifficulty {
    Recruit,
    #[default]
    Officer,
    Admiral,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRoom {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub visibility: RoomVisibility,
    #[serde(default)]
    pub rules: MatchRules,
    #[serde(default)]
    pub balance: BalancePin,
    pub status: RoomStatus,
    #[serde(default)]
    pub host_player_id: Uuid,
    pub players: Vec<Player>,
    pub pending_placements: HashMap<Uuid, Vec<ShipPlacement>>,
    #[serde(default)]
    pub game_id: Option<Uuid>,
    pub game: Option<Game>,
    pub version: u64,
    #[serde(default)]
    pub practice_difficulty: Option<AiDifficulty>,
    #[serde(default)]
    pub matchmaking_quality: Option<MatchmakingQuality>,
    #[serde(default)]
    pub ranked_match: Option<RankedMatchContext>,
    #[serde(default)]
    pub persistence_revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub placement_started_at: Option<DateTime<Utc>>,
    pub disconnected_deadlines: HashMap<Uuid, DateTime<Utc>>,
    pub rematch_requests: HashSet<Uuid>,
    #[serde(default)]
    pub chat_messages: Vec<ChatMessage>,
    #[serde(default)]
    pub ready_resolutions: HashMap<Uuid, PlayerReadyRecord>,
    #[serde(default)]
    pub start_resolutions: HashMap<Uuid, GameStartRecord>,
    #[serde(skip, default)]
    chat_rate_windows: HashMap<Uuid, Vec<DateTime<Utc>>>,
    #[serde(skip, default)]
    chat_blocked_until: HashMap<Uuid, DateTime<Utc>>,
    #[serde(skip, default)]
    last_quick_commands: HashMap<Uuid, (QuickCommandId, DateTime<Utc>)>,
}

impl GameRoom {
    pub fn new(
        code: String,
        name: String,
        visibility: RoomVisibility,
        host_session: &UserSession,
    ) -> Result<Self, GameError> {
        Self::new_with_rules(code, name, visibility, host_session, MatchRules::default())
    }

    pub fn new_with_rules(
        code: String,
        name: String,
        visibility: RoomVisibility,
        host_session: &UserSession,
        rules: MatchRules,
    ) -> Result<Self, GameError> {
        validate_room_name(&name)?;
        let balance = BalancePin::current();
        let rules = rules.validate_for(&balance.manifest)?;
        let now = Utc::now();
        let host = Player::new(host_session, true);
        let host_player_id = host.id;
        let mut room = Self {
            id: Uuid::new_v4(),
            code,
            name,
            visibility,
            rules,
            balance,
            status: RoomStatus::WaitingForOpponent,
            host_player_id,
            players: vec![host],
            pending_placements: HashMap::new(),
            game_id: None,
            game: None,
            version: 1,
            practice_difficulty: None,
            matchmaking_quality: None,
            ranked_match: None,
            persistence_revision: 0,
            created_at: now,
            updated_at: now,
            placement_started_at: None,
            disconnected_deadlines: HashMap::new(),
            rematch_requests: HashSet::new(),
            chat_messages: Vec::new(),
            ready_resolutions: HashMap::new(),
            start_resolutions: HashMap::new(),
            chat_rate_windows: HashMap::new(),
            chat_blocked_until: HashMap::new(),
            last_quick_commands: HashMap::new(),
        };
        room.push_system_message(format!(
            "{} 지휘관이 작전실에 입장했습니다.",
            host_session.nickname
        ));
        Ok(room)
    }

    pub fn join(&mut self, session: &UserSession) -> Result<Uuid, GameError> {
        self.require_executable_balance()?;
        if self.status != RoomStatus::WaitingForOpponent {
            return Err(if self.players.len() >= 2 {
                GameError::RoomFull
            } else {
                GameError::RoomAlreadyStarted
            });
        }
        if self.players.len() >= 2 {
            return Err(GameError::RoomFull);
        }
        if self
            .players
            .iter()
            .any(|player| player.session_id == session.id)
        {
            return Err(GameError::AlreadyJoined);
        }
        if self
            .players
            .iter()
            .any(|player| player.nickname.eq_ignore_ascii_case(&session.nickname))
        {
            return Err(GameError::DuplicateNickname);
        }
        let player = Player::new(session, false);
        let id = player.id;
        self.players.push(player);
        self.status = RoomStatus::WaitingForReady;
        self.bump();
        self.push_system_message(format!(
            "{} 지휘관이 작전실에 입장했습니다.",
            session.nickname
        ));
        Ok(id)
    }

    pub fn player_for_session(&self, session_id: Uuid) -> Result<&Player, GameError> {
        self.players
            .iter()
            .find(|player| player.session_id == session_id)
            .ok_or(GameError::NotRoomMember)
    }

    pub fn configure_practice(
        &mut self,
        human_session_id: Uuid,
        ai_session_id: Uuid,
        difficulty: AiDifficulty,
        ai_placements: Vec<ShipPlacement>,
    ) -> Result<(), GameError> {
        let human = self.player_for_session(human_session_id)?.clone();
        let ai = self.player_for_session(ai_session_id)?.clone();
        let ai_player = self
            .players
            .iter_mut()
            .find(|player| player.id == ai.id)
            .ok_or(GameError::InvalidState)?;
        ai_player.kind = PlayerKind::Ai;
        self.practice_difficulty = Some(difficulty);
        self.set_lobby_ready(human_session_id, Uuid::new_v4(), human.id, true)?;
        self.set_lobby_ready(ai_session_id, Uuid::new_v4(), ai.id, true)?;
        self.start_placement(human_session_id, Uuid::new_v4(), human.id, self.version)?;
        self.place_ships(ai_session_id, ai_placements.clone())?;
        self.confirm_placement(ai_session_id, &ai_placements, 60)?;
        self.push_system_message(format!(
            "AI training opponent connected at {} difficulty.",
            match difficulty {
                AiDifficulty::Recruit => "RECRUIT",
                AiDifficulty::Officer => "OFFICER",
                AiDifficulty::Admiral => "ADMIRAL",
            }
        ));
        Ok(())
    }

    pub fn place_ships(
        &mut self,
        session_id: Uuid,
        placements: Vec<ShipPlacement>,
    ) -> Result<(), GameError> {
        if self.status != RoomStatus::Placement {
            return Err(GameError::InvalidState);
        }
        let player = self.player_for_session(session_id)?;
        if player.placement_confirmed {
            return Err(GameError::PlacementLocked);
        }
        Board::from_placements(&placements)?;
        self.pending_placements.insert(player.id, placements);
        self.bump();
        Ok(())
    }

    pub fn set_lobby_ready(
        &mut self,
        session_id: Uuid,
        request_id: Uuid,
        claimed_player_id: Uuid,
        ready: bool,
    ) -> Result<(PlayerReadyRecord, bool), GameError> {
        let player = self.player_for_session(session_id)?.clone();
        if player.id != claimed_player_id {
            return Err(GameError::Unauthorized);
        }
        if let Some(previous) = self.ready_resolutions.get(&request_id) {
            if previous.player_id == player.id
                && previous.ready_state
                    == if ready {
                        PlayerReadyState::Ready
                    } else {
                        PlayerReadyState::NotReady
                    }
            {
                return Ok((previous.clone(), true));
            }
            return Err(GameError::Unauthorized);
        }
        if self.game_id.is_some()
            || !matches!(
                self.status,
                RoomStatus::WaitingForOpponent
                    | RoomStatus::WaitingForReady
                    | RoomStatus::ReadyToStart
            )
        {
            return Err(GameError::GameAlreadyStarted);
        }

        let next_ready_state = if ready {
            PlayerReadyState::Ready
        } else {
            PlayerReadyState::NotReady
        };
        let changed = player.ready_state != next_ready_state;
        if changed {
            let now = Utc::now();
            if let Some(current) = self
                .players
                .iter_mut()
                .find(|candidate| candidate.id == player.id)
            {
                current.ready_state = next_ready_state;
                current.ready_at = ready.then_some(now);
            }
            self.refresh_lobby_status();
            self.bump();
            self.push_system_message(if ready {
                format!("{} 지휘관이 준비를 완료했습니다.", player.nickname)
            } else {
                format!("{} 지휘관이 준비를 취소했습니다.", player.nickname)
            });
            if ready && self.status == RoomStatus::ReadyToStart {
                self.push_system_message("모든 지휘관의 준비가 완료되었습니다.");
            }
        }

        let record = PlayerReadyRecord {
            request_id,
            room_id: self.id,
            player_id: player.id,
            ready_state: next_ready_state,
            room_state: self.status,
            version: self.version,
            accepted_at: Utc::now(),
        };
        self.remember_ready_resolution(record.clone());
        Ok((record, false))
    }

    pub fn start_placement(
        &mut self,
        session_id: Uuid,
        request_id: Uuid,
        claimed_player_id: Uuid,
        expected_version: u64,
    ) -> Result<(GameStartRecord, bool), GameError> {
        let player = self.player_for_session(session_id)?.clone();
        if player.id != claimed_player_id {
            return Err(GameError::Unauthorized);
        }
        if let Some(previous) = self.start_resolutions.get(&request_id) {
            if previous.started_by == player.id {
                return Ok((previous.clone(), true));
            }
            return Err(GameError::Unauthorized);
        }
        if player.id != self.host_player_id {
            return Err(GameError::NotHost);
        }
        if self.game_id.is_some()
            || matches!(self.status, RoomStatus::Placement | RoomStatus::Playing)
        {
            return Err(GameError::GameAlreadyStarted);
        }
        if self.version != expected_version {
            return Err(GameError::StaleRoomVersion);
        }
        if self.players.len() != 2 {
            return Err(GameError::PlayerCountInvalid);
        }
        if !self
            .players
            .iter()
            .all(|candidate| candidate.ready_state == PlayerReadyState::Ready)
        {
            return Err(GameError::PlayersNotReady);
        }
        if !self
            .players
            .iter()
            .all(|candidate| candidate.connection_state == ConnectionState::Online)
        {
            return Err(GameError::PlayerDisconnected);
        }
        if self.status != RoomStatus::ReadyToStart {
            return Err(GameError::RoomStateInvalid);
        }

        let now = Utc::now();
        let game_id = Uuid::new_v4();
        self.status = RoomStatus::Placement;
        self.game_id = Some(game_id);
        self.placement_started_at = Some(now);
        self.pending_placements.clear();
        for player in &mut self.players {
            player.placement_confirmed = false;
        }
        self.bump();
        self.push_system_message("방장이 작전을 시작했습니다. 함선 배치 채널을 개방합니다.");
        let record = GameStartRecord {
            request_id,
            room_id: self.id,
            game_id,
            started_by: player.id,
            version: self.version,
            started_at: now,
        };
        self.remember_start_resolution(record.clone());
        Ok((record, false))
    }

    pub fn leave(&mut self, session_id: Uuid) -> Result<(), GameError> {
        let leaving_player = self.player_for_session(session_id)?.clone();
        let player_id = leaving_player.id;
        let is_lobby = matches!(
            self.status,
            RoomStatus::WaitingForOpponent | RoomStatus::WaitingForReady | RoomStatus::ReadyToStart
        ) && self.game_id.is_none();

        if is_lobby {
            if player_id == self.host_player_id {
                self.status = RoomStatus::Cancelled;
                if let Some(host) = self
                    .players
                    .iter_mut()
                    .find(|candidate| candidate.id == player_id)
                {
                    host.connection_state = ConnectionState::Offline;
                }
                self.bump();
                self.push_system_message("방장이 작전실을 종료했습니다.");
            } else {
                self.players.retain(|candidate| candidate.id != player_id);
                self.reset_lobby_after_guest_departure();
                self.bump();
                self.push_system_message(format!(
                    "{} 지휘관이 작전실에서 퇴장했습니다.",
                    leaving_player.nickname
                ));
            }
            return Ok(());
        }

        let opponent_id = self
            .players
            .iter()
            .find(|player| player.id != player_id)
            .map(|player| player.id);
        let was_active_game = self.status == RoomStatus::Playing
            && self.game.as_ref().is_some_and(|game| game.result.is_none());
        if was_active_game {
            if let (Some(game), Some(winner_id)) = (self.game.as_mut(), opponent_id) {
                game.forfeit(winner_id, FinishReason::PlayerLeft)?;
                self.status = RoomStatus::Finished;
            } else {
                self.status = RoomStatus::Cancelled;
            }
        } else if !matches!(self.status, RoomStatus::Finished | RoomStatus::Cancelled) {
            self.status = RoomStatus::Cancelled;
        }
        if let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        {
            player.connection_state = ConnectionState::Offline;
        }
        self.bump();
        let message = if was_active_game {
            let winner = self
                .players
                .iter()
                .find(|player| player.id != player_id)
                .map(|player| player.nickname.as_str())
                .unwrap_or("상대");
            format!(
                "Commander {} left the operation. {} 지휘관이 승리했습니다.",
                leaving_player.nickname, winner
            )
        } else {
            format!(
                "{} 지휘관이 작전실에서 퇴장했습니다.",
                leaving_player.nickname
            )
        };
        self.push_system_message(message);
        Ok(())
    }

    pub fn confirm_placement(
        &mut self,
        session_id: Uuid,
        submitted_placements: &[ShipPlacement],
        turn_duration_seconds: u32,
    ) -> Result<bool, GameError> {
        self.require_executable_balance()?;
        if self.status != RoomStatus::Placement || self.game_id.is_none() {
            return Err(GameError::InvalidState);
        }
        let player_id = self.player_for_session(session_id)?.id;
        let stored_placements = self
            .pending_placements
            .get(&player_id)
            .ok_or(GameError::IncompleteFleet)?;
        Board::from_placements_for(submitted_placements, &self.balance.manifest)?;
        if stored_placements != submitted_placements {
            return Err(GameError::PlacementMismatch);
        }
        if self
            .players
            .iter()
            .find(|player| player.id == player_id)
            .is_some_and(|player| player.placement_confirmed)
        {
            return Ok(false);
        }
        if let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        {
            player.placement_confirmed = true;
        }
        self.bump();

        let all_ready =
            self.players.len() == 2 && self.players.iter().all(|player| player.placement_confirmed);
        if all_ready {
            let mut boards = HashMap::new();
            for player in &self.players {
                let placements = self
                    .pending_placements
                    .get(&player.id)
                    .ok_or(GameError::IncompleteFleet)?;
                boards.insert(
                    player.id,
                    Board::from_placements_for(placements, &self.balance.manifest)?,
                );
            }
            self.game = Some(Game::new_with_rules_and_balance(
                boards,
                self.rules,
                turn_duration_seconds,
                self.balance.clone(),
            )?);
            self.status = RoomStatus::Playing;
            self.pending_placements.clear();
            self.bump();
            self.push_system_message("게임이 시작되었습니다. 전투 채널을 개방합니다.");
        }
        Ok(all_ready)
    }

    pub fn fire(
        &mut self,
        session_id: Uuid,
        request_id: Uuid,
        claimed_player_id: Uuid,
        coordinate: Coordinate,
        expected_version: u64,
        expected_turn: u32,
    ) -> Result<(AttackRecord, bool), GameError> {
        self.require_executable_balance()?;
        let player_id = self.player_for_session(session_id)?.id;
        if player_id != claimed_player_id {
            return Err(GameError::Unauthorized);
        }
        if let Some(previous) = self
            .game
            .as_ref()
            .and_then(|game| game.previous_resolution(request_id, player_id))
        {
            return Ok((previous, true));
        }
        if self.status != RoomStatus::Playing {
            return Err(GameError::InvalidState);
        }
        if self.version != expected_version {
            return Err(GameError::VersionConflict);
        }
        let next_version = self.version + 1;
        let record = self.game.as_mut().ok_or(GameError::InvalidState)?.fire(
            request_id,
            player_id,
            coordinate,
            expected_turn,
            next_version,
        )?;
        if record.winner_id.is_some() {
            self.status = RoomStatus::Finished;
        }
        self.bump();
        if let Some(winner_id) = record.winner_id {
            let winner = self
                .players
                .iter()
                .find(|player| player.id == winner_id)
                .map(|player| player.nickname.as_str())
                .unwrap_or("UNKNOWN");
            self.push_system_message(format!(
                "게임이 종료되었습니다. {winner} 지휘관이 적 함대를 전멸시켰습니다."
            ));
        }
        Ok((record, false))
    }

    pub fn request_rematch(&mut self, session_id: Uuid) -> Result<bool, GameError> {
        if self.status != RoomStatus::Finished {
            return Err(GameError::InvalidState);
        }
        let player_id = self.player_for_session(session_id)?.id;
        self.rematch_requests.insert(player_id);
        let accepted = self.rematch_requests.len() == 2;
        if accepted {
            self.game = None;
            self.game_id = None;
            self.placement_started_at = None;
            self.pending_placements.clear();
            self.rematch_requests.clear();
            self.ready_resolutions.clear();
            self.start_resolutions.clear();
            for player in &mut self.players {
                player.placement_confirmed = false;
                player.ready_state = PlayerReadyState::NotReady;
                player.ready_at = None;
            }
            self.status = RoomStatus::WaitingForReady;
        }
        self.bump();
        if accepted {
            self.push_system_message("재대결이 승인되었습니다. 양 지휘관의 준비를 기다립니다.");
        }
        Ok(accepted)
    }

    pub fn surrender(
        &mut self,
        session_id: Uuid,
        claimed_player_id: Uuid,
    ) -> Result<SurrenderRecord, GameError> {
        let surrendering_player = self.player_for_session(session_id)?.clone();
        if surrendering_player.id != claimed_player_id {
            return Err(GameError::Unauthorized);
        }
        let active = self.status == RoomStatus::Playing;
        if !active || self.game.as_ref().is_none_or(|game| game.result.is_some()) {
            return Err(GameError::InvalidState);
        }
        let winner = self
            .players
            .iter()
            .find(|player| player.id != surrendering_player.id)
            .cloned()
            .ok_or(GameError::InvalidState)?;
        self.game
            .as_mut()
            .ok_or(GameError::InvalidState)?
            .forfeit(winner.id, FinishReason::Surrender)?;
        self.status = RoomStatus::Finished;
        self.disconnected_deadlines.clear();
        self.bump();
        let timestamp = self
            .game
            .as_ref()
            .and_then(|game| game.result.as_ref())
            .map(|result| result.finished_at)
            .unwrap_or_else(Utc::now);
        self.push_system_message(format!(
            "Commander {} surrendered. {} 지휘관이 승리했습니다.",
            surrendering_player.nickname, winner.nickname
        ));
        Ok(SurrenderRecord {
            room_id: self.id,
            surrendered_player_id: surrendering_player.id,
            winner_id: winner.id,
            nickname: surrendering_player.nickname,
            timestamp,
        })
    }
}

fn validate_room_name(name: &str) -> Result<(), GameError> {
    let count = name.trim().chars().count();
    if !(2..=32).contains(&count) {
        return Err(GameError::InvalidRoomName);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
