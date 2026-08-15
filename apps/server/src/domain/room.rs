use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::GameError;

use super::{
    ALLOWED_EMOJIS, AttackOutcome, AttackRecord, Board, ChatMessage, ChatMessageType,
    ChatTypingEvent, ConnectionState, Coordinate, FinishReason, Game, GameResult, MAX_CHAT_HISTORY,
    MAX_CHAT_MESSAGE_CHARS, Player, PlayerKind, PlayerReadyState, PlayerRole, QuickCommandId,
    ShipKind, ShipPlacement, SurrenderRecord, TurnExpiration, UserSession,
};

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

pub const RULESET_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRoom {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub visibility: RoomVisibility,
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
        validate_room_name(&name)?;
        let now = Utc::now();
        let host = Player::new(host_session, true);
        let host_player_id = host.id;
        let mut room = Self {
            id: Uuid::new_v4(),
            code,
            name,
            visibility,
            status: RoomStatus::WaitingForOpponent,
            host_player_id,
            players: vec![host],
            pending_placements: HashMap::new(),
            game_id: None,
            game: None,
            version: 1,
            practice_difficulty: None,
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
        if self.status != RoomStatus::Placement || self.game_id.is_none() {
            return Err(GameError::InvalidState);
        }
        let player_id = self.player_for_session(session_id)?.id;
        let stored_placements = self
            .pending_placements
            .get(&player_id)
            .ok_or(GameError::IncompleteFleet)?;
        Board::from_placements(submitted_placements)?;
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
                boards.insert(player.id, Board::from_placements(placements)?);
            }
            self.game = Some(Game::new_with_turn_duration(boards, turn_duration_seconds)?);
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

    pub fn ensure_runtime_state(&mut self, turn_duration_seconds: u32, now: DateTime<Utc>) -> bool {
        let mut changed = false;
        if self.host_player_id.is_nil() {
            if let Some(host) = self
                .players
                .iter()
                .find(|player| player.is_host)
                .or_else(|| self.players.first())
            {
                self.host_player_id = host.id;
                changed = true;
            }
        }
        for player in &mut self.players {
            let expected_role = if player.id == self.host_player_id {
                PlayerRole::Host
            } else {
                PlayerRole::Guest
            };
            if player.role != expected_role || player.is_host != (expected_role == PlayerRole::Host)
            {
                player.role = expected_role;
                player.is_host = expected_role == PlayerRole::Host;
                changed = true;
            }
            if player.ready_state == PlayerReadyState::NotReady && player.ready_at.take().is_some()
            {
                changed = true;
            }
        }
        if self.game_id.is_none() {
            if let Some(game) = &self.game {
                self.game_id = Some(Uuid::new_v4());
                self.placement_started_at.get_or_insert(game.started_at);
                self.status = if game.result.is_some() {
                    RoomStatus::Finished
                } else {
                    RoomStatus::Playing
                };
                changed = true;
            } else if self.status == RoomStatus::Placement {
                self.pending_placements.clear();
                for player in &mut self.players {
                    player.ready_state = PlayerReadyState::NotReady;
                    player.ready_at = None;
                    player.placement_confirmed = false;
                }
                self.refresh_lobby_status();
                changed = true;
            }
        }
        if self.game_id.is_none()
            && matches!(
                self.status,
                RoomStatus::WaitingForOpponent
                    | RoomStatus::WaitingForReady
                    | RoomStatus::ReadyToStart
            )
        {
            let previous = self.status;
            self.refresh_lobby_status();
            changed |= self.status != previous;
        }
        if self.is_active_battle()
            && self
                .game
                .as_mut()
                .is_some_and(|game| game.ensure_turn_timer(turn_duration_seconds, now))
        {
            changed = true;
        }
        if changed {
            self.updated_at = now;
        }
        changed
    }

    pub fn timer_state(&self, now: DateTime<Utc>) -> Option<GameTimerState> {
        let game = self.game.as_ref()?;
        if game.result.is_some() {
            return None;
        }
        Some(GameTimerState {
            room_id: self.id,
            game_id: self.game_id.unwrap_or(self.id),
            turn_number: game.turn_number,
            active_player_id: game.current_player_id,
            game_started_at: game.started_at,
            turn_started_at: game.turn_started_at,
            turn_deadline_at: game.turn_deadline_at,
            turn_duration_seconds: game.turn_duration_seconds,
            server_timestamp: now,
        })
    }

    pub fn expire_turn(
        &mut self,
        expected_turn: u32,
        expected_player_id: Uuid,
        expected_deadline: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Option<TurnExpiredRecord>, GameError> {
        if !self.is_active_battle() {
            return Ok(None);
        }
        let expiration = self
            .game
            .as_mut()
            .ok_or(GameError::InvalidState)?
            .expire_turn(expected_turn, expected_player_id, expected_deadline, now)?;
        let Some(expiration) = expiration else {
            return Ok(None);
        };
        if expiration.winner_id.is_some() {
            self.status = RoomStatus::Finished;
            self.disconnected_deadlines.clear();
        }
        self.bump();
        let nickname = self
            .players
            .iter()
            .find(|player| player.id == expiration.expired_player_id)
            .map(|player| player.nickname.as_str())
            .unwrap_or("상대");
        let message = if expiration.winner_id.is_some() {
            format!(
                "{} 지휘관이 3회 연속 시간 초과로 자동 기권 처리되었습니다.",
                nickname
            )
        } else {
            format!(
                "{} 지휘관의 작전 시간이 만료되었습니다. 공격 기회가 소멸했습니다.",
                nickname
            )
        };
        self.push_system_message(message);
        Ok(Some(TurnExpiredRecord::from_expiration(
            self.id,
            self.game_id.unwrap_or(self.id),
            expiration,
            now,
        )))
    }

    fn is_active_battle(&self) -> bool {
        self.status == RoomStatus::Playing
    }

    pub fn disconnect(
        &mut self,
        session_id: Uuid,
        grace_seconds: i64,
    ) -> Result<DateTime<Utc>, GameError> {
        if matches!(self.status, RoomStatus::Finished | RoomStatus::Cancelled) {
            return Err(GameError::InvalidState);
        }
        let disconnected_player = self.player_for_session(session_id)?.clone();
        let player_id = disconnected_player.id;
        if let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        {
            player.connection_state = ConnectionState::Reconnecting;
        }
        let deadline = Utc::now() + Duration::seconds(grace_seconds);
        self.disconnected_deadlines.insert(player_id, deadline);
        self.bump();
        self.push_system_message(format!(
            "{} 지휘관의 연결이 끊겼습니다. 재접속을 기다립니다.",
            disconnected_player.nickname
        ));
        Ok(deadline)
    }

    pub fn reconnect(&mut self, session_id: Uuid) -> Result<bool, GameError> {
        let reconnecting_player = self.player_for_session(session_id)?.clone();
        let player_id = reconnecting_player.id;
        let was_reconnecting = reconnecting_player.connection_state != ConnectionState::Online
            || self.disconnected_deadlines.contains_key(&player_id);
        if !was_reconnecting {
            return Ok(false);
        }
        if let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        {
            player.connection_state = ConnectionState::Online;
        }
        self.disconnected_deadlines.remove(&player_id);
        self.bump();
        self.push_system_message(format!(
            "{} 지휘관이 전투 채널에 재접속했습니다.",
            reconnecting_player.nickname
        ));
        Ok(true)
    }

    pub fn expire_disconnect(
        &mut self,
        player_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<bool, GameError> {
        let Some(deadline) = self.disconnected_deadlines.get(&player_id).copied() else {
            return Ok(false);
        };
        if deadline > now {
            return Ok(false);
        }
        let disconnected_player = self
            .players
            .iter()
            .find(|player| player.id == player_id)
            .cloned()
            .ok_or(GameError::NotRoomMember)?;
        let disconnected_nickname = disconnected_player.nickname.clone();
        if let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        {
            player.connection_state = ConnectionState::Offline;
        }
        let opponent_id = self
            .players
            .iter()
            .find(|player| player.id != player_id)
            .map(|player| player.id);
        let is_lobby = matches!(
            self.status,
            RoomStatus::WaitingForOpponent | RoomStatus::WaitingForReady | RoomStatus::ReadyToStart
        ) && self.game_id.is_none();
        if is_lobby && player_id != self.host_player_id {
            self.players.retain(|player| player.id != player_id);
            self.reset_lobby_after_guest_departure();
        } else if is_lobby {
            self.status = RoomStatus::Cancelled;
        } else if let (Some(game), Some(winner_id)) = (self.game.as_mut(), opponent_id) {
            game.forfeit(winner_id, FinishReason::DisconnectTimeout)?;
            self.status = RoomStatus::Finished;
        } else {
            self.status = RoomStatus::Cancelled;
        }
        self.disconnected_deadlines.remove(&player_id);
        self.bump();
        let message = if is_lobby && player_id != self.host_player_id {
            format!(
                "{} 지휘관의 재접속 시간이 만료되어 자리에서 제거되었습니다.",
                disconnected_nickname
            )
        } else if is_lobby {
            "방장의 재접속 시간이 만료되어 작전실이 종료되었습니다.".to_string()
        } else if let Some(winner_id) = opponent_id {
            let winner = self
                .players
                .iter()
                .find(|player| player.id == winner_id)
                .map(|player| player.nickname.as_str())
                .unwrap_or("상대");
            format!(
                "{} 지휘관의 재접속 시간이 만료되었습니다. {} 지휘관이 승리했습니다.",
                disconnected_nickname, winner
            )
        } else {
            format!(
                "{} 지휘관의 재접속 시간이 만료되어 작전이 취소되었습니다.",
                disconnected_nickname
            )
        };
        self.push_system_message(message);
        Ok(true)
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

    pub fn send_chat(
        &mut self,
        session_id: Uuid,
        client_message_id: Uuid,
        message_type: ChatMessageType,
        content: Option<String>,
        command_id: Option<QuickCommandId>,
    ) -> Result<(ChatMessage, bool), GameError> {
        self.send_chat_at(
            session_id,
            client_message_id,
            message_type,
            content,
            command_id,
            Utc::now(),
        )
    }

    fn send_chat_at(
        &mut self,
        session_id: Uuid,
        client_message_id: Uuid,
        message_type: ChatMessageType,
        content: Option<String>,
        command_id: Option<QuickCommandId>,
        now: DateTime<Utc>,
    ) -> Result<(ChatMessage, bool), GameError> {
        if !matches!(
            self.status,
            RoomStatus::WaitingForOpponent
                | RoomStatus::WaitingForReady
                | RoomStatus::ReadyToStart
                | RoomStatus::Placement
                | RoomStatus::Playing
                | RoomStatus::Finished
        ) {
            return Err(GameError::InvalidState);
        }
        let player = self.player_for_session(session_id)?.clone();
        if let Some(previous) = self
            .chat_messages
            .iter()
            .find(|message| message.message_id == client_message_id)
        {
            if previous.player_id == Some(player.id) {
                return Ok((previous.clone(), true));
            }
            return Err(GameError::Unauthorized);
        }
        let (normalized, resolved_command) =
            match message_type {
                ChatMessageType::Text => (
                    normalize_chat_message(content.ok_or(GameError::InvalidChatMessage)?)?,
                    None,
                ),
                ChatMessageType::Emoji => {
                    let emoji = content.ok_or(GameError::InvalidEmoji)?;
                    if command_id.is_some() || !ALLOWED_EMOJIS.contains(&emoji.as_str()) {
                        return Err(GameError::InvalidEmoji);
                    }
                    (emoji, None)
                }
                ChatMessageType::QuickCommand => {
                    if content.is_some() {
                        return Err(GameError::InvalidQuickCommand);
                    }
                    let command = command_id.ok_or(GameError::InvalidQuickCommand)?;
                    if self.last_quick_commands.get(&player.id).is_some_and(
                        |(previous, sent_at)| {
                            *previous == command
                                && now.signed_duration_since(*sent_at).num_milliseconds() < 2_000
                        },
                    ) {
                        return Err(GameError::RateLimited);
                    }
                    (command.label().to_string(), Some(command))
                }
                ChatMessageType::System => return Err(GameError::InvalidChatMessage),
            };
        if self
            .chat_blocked_until
            .get(&player.id)
            .is_some_and(|blocked_until| *blocked_until > now)
        {
            return Err(GameError::RateLimited);
        }
        let window = self.chat_rate_windows.entry(player.id).or_default();
        window.retain(|sent_at| now.signed_duration_since(*sent_at).num_seconds() < 10);
        let recent_two_seconds = window
            .iter()
            .filter(|sent_at| now.signed_duration_since(**sent_at).num_milliseconds() < 2_000)
            .count();
        if window.len() >= 8 || recent_two_seconds >= 3 {
            self.chat_blocked_until
                .insert(player.id, now + Duration::seconds(3));
            return Err(GameError::RateLimited);
        }
        window.push(now);
        if let Some(command) = resolved_command {
            self.last_quick_commands.insert(player.id, (command, now));
        }
        let message = ChatMessage {
            message_id: client_message_id,
            room_id: self.id,
            player_id: Some(player.id),
            nickname: player.nickname,
            content: normalized,
            timestamp: now,
            message_type,
            command_id: resolved_command,
        };
        self.append_chat_message(message.clone());
        Ok((message, false))
    }

    pub fn chat_history(&self, session_id: Uuid) -> Result<Vec<ChatMessage>, GameError> {
        self.player_for_session(session_id)?;
        Ok(self.chat_messages.clone())
    }

    pub fn record_start_rejection(
        &mut self,
        session_id: Uuid,
        error_code: &str,
    ) -> Result<ChatMessage, GameError> {
        let player = self.player_for_session(session_id)?;
        let nickname = player.nickname.clone();
        Ok(self.push_system_message(format!(
            "{} 지휘관의 게임 시작 요청이 거부되었습니다. ({})",
            nickname, error_code
        )))
    }

    pub fn typing_event(
        &self,
        session_id: Uuid,
        is_typing: bool,
    ) -> Result<ChatTypingEvent, GameError> {
        if self.status == RoomStatus::Cancelled {
            return Err(GameError::InvalidState);
        }
        let player = self.player_for_session(session_id)?;
        Ok(ChatTypingEvent {
            room_id: self.id,
            player_id: player.id,
            nickname: player.nickname.clone(),
            is_typing,
        })
    }

    fn push_system_message(&mut self, message: impl Into<String>) -> ChatMessage {
        let message = ChatMessage {
            message_id: Uuid::new_v4(),
            room_id: self.id,
            player_id: None,
            nickname: "SYSTEM".to_string(),
            content: message.into(),
            timestamp: Utc::now(),
            message_type: ChatMessageType::System,
            command_id: None,
        };
        self.append_chat_message(message.clone());
        message
    }

    fn append_chat_message(&mut self, message: ChatMessage) {
        self.chat_messages.push(message);
        if self.chat_messages.len() > MAX_CHAT_HISTORY {
            let excess = self.chat_messages.len() - MAX_CHAT_HISTORY;
            self.chat_messages.drain(..excess);
        }
        self.updated_at = Utc::now();
    }

    fn refresh_lobby_status(&mut self) {
        self.status = if self.players.len() < 2 {
            RoomStatus::WaitingForOpponent
        } else if self
            .players
            .iter()
            .all(|player| player.ready_state == PlayerReadyState::Ready)
        {
            RoomStatus::ReadyToStart
        } else {
            RoomStatus::WaitingForReady
        };
    }

    fn reset_lobby_after_guest_departure(&mut self) {
        self.pending_placements.clear();
        self.game = None;
        self.game_id = None;
        self.placement_started_at = None;
        self.ready_resolutions.clear();
        self.start_resolutions.clear();
        self.rematch_requests.clear();
        self.disconnected_deadlines.clear();
        for player in &mut self.players {
            player.ready_state = PlayerReadyState::NotReady;
            player.ready_at = None;
            player.placement_confirmed = false;
        }
        self.status = RoomStatus::WaitingForOpponent;
    }

    fn remember_ready_resolution(&mut self, record: PlayerReadyRecord) {
        if self.ready_resolutions.len() >= 128 {
            if let Some(oldest) = self
                .ready_resolutions
                .iter()
                .min_by_key(|(_, resolution)| resolution.accepted_at)
                .map(|(request_id, _)| *request_id)
            {
                self.ready_resolutions.remove(&oldest);
            }
        }
        self.ready_resolutions.insert(record.request_id, record);
    }

    fn remember_start_resolution(&mut self, record: GameStartRecord) {
        if self.start_resolutions.len() >= 64 {
            if let Some(oldest) = self
                .start_resolutions
                .iter()
                .min_by_key(|(_, resolution)| resolution.started_at)
                .map(|(request_id, _)| *request_id)
            {
                self.start_resolutions.remove(&oldest);
            }
        }
        self.start_resolutions.insert(record.request_id, record);
    }

    pub fn can_start_game(&self) -> bool {
        self.status == RoomStatus::ReadyToStart
            && self.game_id.is_none()
            && self.players.len() == 2
            && self
                .players
                .iter()
                .all(|player| player.ready_state == PlayerReadyState::Ready)
            && self
                .players
                .iter()
                .all(|player| player.connection_state == ConnectionState::Online)
    }

    fn bump(&mut self) {
        self.version += 1;
        self.updated_at = Utc::now();
    }

    pub fn summary(&self) -> RoomSummary {
        RoomSummary {
            id: self.id,
            code: self.code.clone(),
            name: self.name.clone(),
            status: self.status,
            host_player_id: self.host_player_id,
            game_id: self.game_id,
            version: self.version,
            player_count: self.players.len() as u8,
            capacity: 2,
            created_at: self.created_at,
        }
    }

    pub fn snapshot_for(&self, session_id: Uuid) -> Result<GameSnapshot, GameError> {
        let me = self.player_for_session(session_id)?;
        let players = self
            .players
            .iter()
            .map(|player| PlayerPublic::from_player(player, self.game.as_ref()))
            .collect();
        let (own_board, target_board, revealed_board, turn_number, current_player_id, result) =
            if let Some(game) = &self.game {
                let board = game.boards.get(&me.id).ok_or(GameError::InvalidState)?;
                let own = OwnBoardSnapshot {
                    ships: board
                        .ships()
                        .iter()
                        .map(|ship| OwnShipSnapshot {
                            kind: ship.kind,
                            cells: ship.cells.clone(),
                            hits: ship.hits.iter().copied().collect(),
                            sunk: ship.is_sunk(),
                        })
                        .collect(),
                    attacks_received: board
                        .attacks_received()
                        .iter()
                        .map(|attack| CellAttackSnapshot {
                            coordinate: attack.coordinate,
                            outcome: attack.outcome,
                        })
                        .collect(),
                };
                let target = TargetBoardSnapshot {
                    attacks: game
                        .attacks
                        .iter()
                        .filter(|attack| attack.attacker_id == me.id)
                        .map(|attack| TargetAttackSnapshot {
                            coordinate: attack.coordinate,
                            outcome: attack.outcome,
                            sunk_ship: attack.sunk_ship,
                        })
                        .collect(),
                };
                let revealed = if self.status == RoomStatus::Finished {
                    game.boards
                        .iter()
                        .find(|(player_id, _)| **player_id != me.id)
                        .map(|(_, board)| OwnBoardSnapshot {
                            ships: board
                                .ships()
                                .iter()
                                .map(|ship| OwnShipSnapshot {
                                    kind: ship.kind,
                                    cells: ship.cells.clone(),
                                    hits: ship.hits.iter().copied().collect(),
                                    sunk: ship.is_sunk(),
                                })
                                .collect(),
                            attacks_received: board
                                .attacks_received()
                                .iter()
                                .map(|attack| CellAttackSnapshot {
                                    coordinate: attack.coordinate,
                                    outcome: attack.outcome,
                                })
                                .collect(),
                        })
                } else {
                    None
                };
                (
                    Some(own),
                    Some(target),
                    revealed,
                    Some(game.turn_number),
                    Some(game.current_player_id),
                    game.result.clone(),
                )
            } else {
                (None, None, None, None, None, None)
            };

        Ok(GameSnapshot {
            protocol_version: crate::PROTOCOL_VERSION,
            room: self.summary(),
            room_id: self.id,
            room_state: self.status,
            host_player_id: self.host_player_id,
            game_id: self.game_id,
            can_start_game: self.can_start_game() && me.id == self.host_player_id,
            room_version: self.version,
            version: self.version,
            self_player_id: me.id,
            players,
            practice_difficulty: self.practice_difficulty,
            own_board,
            target_board,
            revealed_board,
            turn_number,
            current_player_id,
            result,
            reconnect_deadline: self.disconnected_deadlines.values().min().copied(),
            rematch_requested_by: self.rematch_requests.iter().copied().collect(),
            placement: self.pending_placements.get(&me.id).cloned(),
            placement_started_at: self.placement_started_at,
            game_started_at: self.game.as_ref().map(|game| game.started_at),
            game_finished_at: self
                .game
                .as_ref()
                .and_then(|game| game.result.as_ref().map(|result| result.finished_at)),
            turn_started_at: self.game.as_ref().and_then(|game| game.turn_started_at),
            turn_deadline_at: self.game.as_ref().and_then(|game| game.turn_deadline_at),
            turn_duration_seconds: self.game.as_ref().map(|game| game.turn_duration_seconds),
            server_timestamp: Utc::now(),
        })
    }

    pub fn replay_for(&self, session_id: Uuid) -> Result<GameReplay, GameError> {
        self.player_for_session(session_id)?;
        if self.status != RoomStatus::Finished {
            return Err(GameError::InvalidState);
        }
        let game = self.game.as_ref().ok_or(GameError::InvalidState)?;
        let result = game.result.clone().ok_or(GameError::InvalidState)?;
        let players = self
            .players
            .iter()
            .map(|player| {
                let board = game.boards.get(&player.id).ok_or(GameError::InvalidState)?;
                Ok(ReplayPlayer {
                    id: player.id,
                    nickname: player.nickname.clone(),
                    kind: player.kind,
                    fleet: board
                        .ships()
                        .iter()
                        .map(|ship| ReplayShip {
                            kind: ship.kind,
                            cells: ship.cells.clone(),
                        })
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, GameError>>()?;
        let timeline = if game.timeline.is_empty() {
            game.attacks
                .iter()
                .cloned()
                .map(GameTimelineEvent::Attack)
                .collect()
        } else {
            game.timeline.clone()
        };
        Ok(GameReplay {
            protocol_version: crate::PROTOCOL_VERSION,
            ruleset_version: RULESET_VERSION,
            room_id: self.id,
            room_name: self.name.clone(),
            game_id: self.game_id.ok_or(GameError::InvalidState)?,
            first_player_id: game.first_player_id,
            started_at: game.started_at,
            finished_at: result.finished_at,
            players,
            timeline,
            result,
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

fn normalize_chat_message(message: String) -> Result<String, GameError> {
    let normalized = message.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim();
    let count = trimmed.chars().count();
    let safe = (1..=MAX_CHAT_MESSAGE_CHARS).contains(&count)
        && !trimmed.contains(['<', '>'])
        && trimmed
            .chars()
            .all(|character| !character.is_control() || character == '\n');
    if safe {
        Ok(trimmed.to_string())
    } else {
        Err(GameError::InvalidChatMessage)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomSummary {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub status: RoomStatus,
    pub host_player_id: Uuid,
    pub game_id: Option<Uuid>,
    pub version: u64,
    pub player_count: u8,
    pub capacity: u8,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerPublic {
    pub id: Uuid,
    pub nickname: String,
    pub kind: PlayerKind,
    pub role: PlayerRole,
    pub is_host: bool,
    pub placement_confirmed: bool,
    pub ready_state: PlayerReadyState,
    pub joined_at: DateTime<Utc>,
    pub ready_at: Option<DateTime<Utc>>,
    pub consecutive_timeout_count: u8,
    pub total_timeout_count: u32,
    pub connection_state: ConnectionState,
}

impl PlayerPublic {
    fn from_player(player: &Player, game: Option<&Game>) -> Self {
        Self {
            id: player.id,
            nickname: player.nickname.clone(),
            kind: player.kind,
            role: player.role,
            is_host: player.is_host,
            placement_confirmed: player.placement_confirmed,
            ready_state: player.ready_state,
            joined_at: player.joined_at,
            ready_at: player.ready_at,
            consecutive_timeout_count: game
                .and_then(|game| game.consecutive_timeout_counts.get(&player.id).copied())
                .unwrap_or_default(),
            total_timeout_count: game
                .and_then(|game| game.total_timeout_counts.get(&player.id).copied())
                .unwrap_or_default(),
            connection_state: player.connection_state,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSnapshot {
    pub protocol_version: u16,
    pub room: RoomSummary,
    pub room_id: Uuid,
    pub room_state: RoomStatus,
    pub host_player_id: Uuid,
    pub game_id: Option<Uuid>,
    pub can_start_game: bool,
    pub room_version: u64,
    pub version: u64,
    pub self_player_id: Uuid,
    pub players: Vec<PlayerPublic>,
    pub practice_difficulty: Option<AiDifficulty>,
    pub own_board: Option<OwnBoardSnapshot>,
    pub target_board: Option<TargetBoardSnapshot>,
    pub revealed_board: Option<OwnBoardSnapshot>,
    pub turn_number: Option<u32>,
    pub current_player_id: Option<Uuid>,
    pub result: Option<GameResult>,
    pub reconnect_deadline: Option<DateTime<Utc>>,
    pub rematch_requested_by: Vec<Uuid>,
    pub placement: Option<Vec<ShipPlacement>>,
    pub placement_started_at: Option<DateTime<Utc>>,
    pub game_started_at: Option<DateTime<Utc>>,
    pub game_finished_at: Option<DateTime<Utc>>,
    pub turn_started_at: Option<DateTime<Utc>>,
    pub turn_deadline_at: Option<DateTime<Utc>>,
    pub turn_duration_seconds: Option<u32>,
    pub server_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnBoardSnapshot {
    pub ships: Vec<OwnShipSnapshot>,
    pub attacks_received: Vec<CellAttackSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnShipSnapshot {
    pub kind: ShipKind,
    pub cells: Vec<Coordinate>,
    pub hits: Vec<Coordinate>,
    pub sunk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellAttackSnapshot {
    pub coordinate: Coordinate,
    pub outcome: AttackOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetBoardSnapshot {
    pub attacks: Vec<TargetAttackSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetAttackSnapshot {
    pub coordinate: Coordinate,
    pub outcome: AttackOutcome,
    pub sunk_ship: Option<ShipKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayShip {
    pub kind: ShipKind,
    pub cells: Vec<Coordinate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayPlayer {
    pub id: Uuid,
    pub nickname: String,
    pub kind: PlayerKind,
    pub fleet: Vec<ReplayShip>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameReplay {
    pub protocol_version: u16,
    pub ruleset_version: u16,
    pub room_id: Uuid,
    pub room_name: String,
    pub game_id: Uuid,
    pub first_player_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub players: Vec<ReplayPlayer>,
    pub timeline: Vec<GameTimelineEvent>,
    pub result: GameResult,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Orientation;

    fn session(nickname: &str) -> UserSession {
        let now = Utc::now();
        UserSession {
            id: Uuid::new_v4(),
            nickname: nickname.to_string(),
            token_hash: Uuid::new_v4().to_string(),
            created_at: now,
            last_seen_at: now,
            current_room_id: None,
        }
    }

    fn fleet(first_row: u8) -> Vec<ShipPlacement> {
        vec![
            ShipPlacement {
                kind: ShipKind::Carrier,
                origin: Coordinate {
                    row: first_row,
                    col: 0,
                },
                orientation: Orientation::Horizontal,
            },
            ShipPlacement {
                kind: ShipKind::Battleship,
                origin: Coordinate {
                    row: first_row + 1,
                    col: 0,
                },
                orientation: Orientation::Horizontal,
            },
            ShipPlacement {
                kind: ShipKind::Cruiser,
                origin: Coordinate {
                    row: first_row + 2,
                    col: 0,
                },
                orientation: Orientation::Horizontal,
            },
            ShipPlacement {
                kind: ShipKind::Submarine,
                origin: Coordinate {
                    row: first_row + 3,
                    col: 0,
                },
                orientation: Orientation::Horizontal,
            },
            ShipPlacement {
                kind: ShipKind::Destroyer,
                origin: Coordinate {
                    row: first_row + 4,
                    col: 0,
                },
                orientation: Orientation::Horizontal,
            },
        ]
    }

    fn playing_room() -> (GameRoom, UserSession, UserSession) {
        let first = session("Alpha");
        let second = session("Bravo");
        let mut room = GameRoom::new(
            "ABC234".to_string(),
            "Test operation".to_string(),
            RoomVisibility::Private,
            &first,
        )
        .unwrap();
        room.join(&second).unwrap();
        prepare_placement(&mut room, &first, &second);
        room.place_ships(first.id, fleet(0)).unwrap();
        room.place_ships(second.id, fleet(5)).unwrap();
        assert!(!room.confirm_placement(first.id, &fleet(0), 60).unwrap());
        assert!(room.confirm_placement(second.id, &fleet(5), 60).unwrap());
        (room, first, second)
    }

    fn prepare_placement(room: &mut GameRoom, host: &UserSession, guest: &UserSession) {
        let host_player_id = room.player_for_session(host.id).unwrap().id;
        let guest_player_id = room.player_for_session(guest.id).unwrap().id;
        room.set_lobby_ready(host.id, Uuid::new_v4(), host_player_id, true)
            .unwrap();
        room.set_lobby_ready(guest.id, Uuid::new_v4(), guest_player_id, true)
            .unwrap();
        room.start_placement(host.id, Uuid::new_v4(), host_player_id, room.version)
            .unwrap();
    }

    #[test]
    fn follows_waiting_ready_start_placement_playing_state_machine() {
        let host = session("Alpha");
        let guest = session("Bravo");
        let mut room = GameRoom::new(
            "ABC234".to_string(),
            "Test operation".to_string(),
            RoomVisibility::Public,
            &host,
        )
        .unwrap();
        assert_eq!(room.status, RoomStatus::WaitingForOpponent);
        room.join(&guest).unwrap();
        assert_eq!(room.status, RoomStatus::WaitingForReady);
        let host_player_id = room.player_for_session(host.id).unwrap().id;
        let guest_player_id = room.player_for_session(guest.id).unwrap().id;
        room.set_lobby_ready(host.id, Uuid::new_v4(), host_player_id, true)
            .unwrap();
        assert_eq!(room.status, RoomStatus::WaitingForReady);
        room.set_lobby_ready(guest.id, Uuid::new_v4(), guest_player_id, true)
            .unwrap();
        assert_eq!(room.status, RoomStatus::ReadyToStart);
        assert!(room.game_id.is_none());
        assert!(room.game.is_none());
        assert!(room.snapshot_for(host.id).unwrap().can_start_game);
        assert!(!room.snapshot_for(guest.id).unwrap().can_start_game);
        room.start_placement(host.id, Uuid::new_v4(), host_player_id, room.version)
            .unwrap();
        assert_eq!(room.status, RoomStatus::Placement);
        assert_eq!(
            room.confirm_placement(host.id, &fleet(0), 60).unwrap_err(),
            GameError::IncompleteFleet
        );
        room.place_ships(host.id, fleet(0)).unwrap();
        room.place_ships(guest.id, fleet(5)).unwrap();
        assert!(!room.confirm_placement(host.id, &fleet(0), 60).unwrap());
        assert!(room.confirm_placement(guest.id, &fleet(5), 60).unwrap());
        assert_eq!(room.status, RoomStatus::Playing);
        assert!(room.game.is_some());
        assert_eq!(
            room.place_ships(host.id, fleet(0)).unwrap_err(),
            GameError::InvalidState
        );
    }

    #[test]
    fn lobby_departures_reset_a_guest_slot_and_host_departure_cancels_the_room() {
        let host = session("Alpha");
        let guest = session("Bravo");
        let mut room = GameRoom::new(
            "ABC234".to_string(),
            "Test operation".to_string(),
            RoomVisibility::Private,
            &host,
        )
        .unwrap();
        let host_player_id = room.player_for_session(host.id).unwrap().id;
        room.set_lobby_ready(host.id, Uuid::new_v4(), host_player_id, true)
            .unwrap();
        assert_eq!(room.status, RoomStatus::WaitingForOpponent);
        assert_eq!(
            room.start_placement(host.id, Uuid::new_v4(), host_player_id, room.version)
                .unwrap_err(),
            GameError::PlayerCountInvalid
        );
        room.join(&guest).unwrap();
        room.leave(guest.id).unwrap();
        assert_eq!(room.status, RoomStatus::WaitingForOpponent);
        assert_eq!(room.players.len(), 1);
        assert_eq!(
            room.player_for_session(host.id).unwrap().ready_state,
            PlayerReadyState::NotReady
        );

        room.join(&guest).unwrap();
        room.leave(host.id).unwrap();
        assert_eq!(room.status, RoomStatus::Cancelled);
        assert_eq!(
            room.chat_messages.last().unwrap().content,
            "방장이 작전실을 종료했습니다."
        );
    }

    #[tokio::test]
    async fn concurrent_start_and_unready_allow_only_one_state_transition() {
        let host = session("Alpha");
        let guest = session("Bravo");
        let mut room = GameRoom::new(
            "ABC234".to_string(),
            "Test operation".to_string(),
            RoomVisibility::Private,
            &host,
        )
        .unwrap();
        room.join(&guest).unwrap();
        let host_player_id = room.player_for_session(host.id).unwrap().id;
        let guest_player_id = room.player_for_session(guest.id).unwrap().id;
        room.set_lobby_ready(host.id, Uuid::new_v4(), host_player_id, true)
            .unwrap();
        room.set_lobby_ready(guest.id, Uuid::new_v4(), guest_player_id, true)
            .unwrap();
        let shared = std::sync::Arc::new(tokio::sync::Mutex::new(room));
        let expected_version = shared.lock().await.version;

        let start_room = shared.clone();
        let start = tokio::spawn(async move {
            start_room.lock().await.start_placement(
                host.id,
                Uuid::new_v4(),
                host_player_id,
                expected_version,
            )
        });
        let unready_room = shared.clone();
        let unready = tokio::spawn(async move {
            unready_room.lock().await.set_lobby_ready(
                guest.id,
                Uuid::new_v4(),
                guest_player_id,
                false,
            )
        });

        let start_result = start.await.unwrap();
        let unready_result = unready.await.unwrap();
        assert_ne!(start_result.is_ok(), unready_result.is_ok());
        let room = shared.lock().await;
        if start_result.is_ok() {
            assert_eq!(room.status, RoomStatus::Placement);
            assert_eq!(unready_result.unwrap_err(), GameError::GameAlreadyStarted);
        } else {
            assert_eq!(room.status, RoomStatus::WaitingForReady);
            assert_eq!(start_result.unwrap_err(), GameError::PlayersNotReady);
        }
    }

    #[test]
    fn legacy_auto_placement_state_is_migrated_back_to_the_lobby() {
        let host = session("Alpha");
        let guest = session("Bravo");
        let mut room = GameRoom::new(
            "ABC234".to_string(),
            "Test operation".to_string(),
            RoomVisibility::Private,
            &host,
        )
        .unwrap();
        room.join(&guest).unwrap();
        room.status = serde_json::from_str::<RoomStatus>("\"DISCONNECTED\"").unwrap();
        room.pending_placements.insert(room.players[0].id, fleet(0));
        room.players[0].placement_confirmed = true;
        room.players[0].ready_state = PlayerReadyState::Ready;
        room.players[0].ready_at = Some(Utc::now());

        assert!(room.ensure_runtime_state(60, Utc::now()));
        assert_eq!(room.status, RoomStatus::WaitingForReady);
        assert!(room.pending_placements.is_empty());
        assert!(room.game_id.is_none());
        assert!(room.players.iter().all(|player| {
            player.ready_state == PlayerReadyState::NotReady
                && !player.placement_confirmed
                && player.ready_at.is_none()
        }));
    }

    #[test]
    fn personalized_snapshot_never_contains_opponent_ships_or_session_ids() {
        let (room, first, second) = playing_room();
        let first_snapshot = serde_json::to_value(room.snapshot_for(first.id).unwrap()).unwrap();
        let second_snapshot = serde_json::to_value(room.snapshot_for(second.id).unwrap()).unwrap();

        assert_eq!(first_snapshot["protocolVersion"], crate::PROTOCOL_VERSION);
        assert!(first_snapshot["ownBoard"]["ships"].is_array());
        assert!(second_snapshot["ownBoard"]["ships"].is_array());
        assert!(first_snapshot["targetBoard"].get("ships").is_none());
        assert!(second_snapshot["targetBoard"].get("ships").is_none());
        let first_json = serde_json::to_string(&first_snapshot).unwrap();
        assert!(!first_json.contains("sessionId"));
        assert!(!first_json.contains(&second.id.to_string()));
    }

    #[test]
    fn duplicate_attack_is_idempotent_even_with_stale_version() {
        let (mut room, first, second) = playing_room();
        let current_id = room.game.as_ref().unwrap().current_player_id;
        let (session_id, player_id) = if room.player_for_session(first.id).unwrap().id == current_id
        {
            (first.id, current_id)
        } else {
            (second.id, current_id)
        };
        let request_id = Uuid::new_v4();
        let version = room.version;
        let (original, duplicate) = room
            .fire(
                session_id,
                request_id,
                player_id,
                Coordinate { row: 9, col: 9 },
                version,
                1,
            )
            .unwrap();
        assert!(!duplicate);
        let resolved_version = room.version;
        let (replayed, duplicate) = room
            .fire(
                session_id,
                request_id,
                player_id,
                Coordinate { row: 9, col: 9 },
                version,
                1,
            )
            .unwrap();
        assert!(duplicate);
        assert_eq!(original.request_id, replayed.request_id);
        assert_eq!(room.version, resolved_version);
    }

    #[test]
    fn reconnect_restores_the_previous_state_and_expiry_forfeits() {
        let (mut room, first, second) = playing_room();
        room.disconnect(first.id, 90).unwrap();
        assert_eq!(room.status, RoomStatus::Playing);
        assert_eq!(
            room.player_for_session(first.id).unwrap().connection_state,
            ConnectionState::Reconnecting
        );
        room.reconnect(first.id).unwrap();
        assert_eq!(room.status, RoomStatus::Playing);

        let first_player_id = room.player_for_session(first.id).unwrap().id;
        let second_player_id = room.player_for_session(second.id).unwrap().id;
        room.disconnect(first.id, 0).unwrap();
        assert!(
            room.expire_disconnect(first_player_id, Utc::now() + Duration::seconds(1))
                .unwrap()
        );
        assert_eq!(room.status, RoomStatus::Finished);
        assert_eq!(
            room.game
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap()
                .winner_id,
            second_player_id
        );
        assert_eq!(
            room.game
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap()
                .win_type,
            crate::domain::WinType::Disconnect
        );
        assert_eq!(
            room.disconnect(second.id, 90).unwrap_err(),
            GameError::InvalidState
        );
    }

    #[test]
    fn surrender_finishes_once_and_records_the_win_type() {
        let (mut room, first, second) = playing_room();
        let first_player_id = room.player_for_session(first.id).unwrap().id;
        let second_player_id = room.player_for_session(second.id).unwrap().id;

        assert_eq!(
            room.surrender(first.id, second_player_id).unwrap_err(),
            GameError::Unauthorized
        );
        let record = room.surrender(first.id, first_player_id).unwrap();
        assert_eq!(record.surrendered_player_id, first_player_id);
        assert_eq!(record.winner_id, second_player_id);
        assert_eq!(room.status, RoomStatus::Finished);
        let result = room.game.as_ref().unwrap().result.as_ref().unwrap();
        assert_eq!(result.finish_reason, FinishReason::Surrender);
        assert_eq!(result.win_type, crate::domain::WinType::Surrender);
        assert!(
            room.chat_messages
                .last()
                .unwrap()
                .content
                .contains("Commander Alpha surrendered")
        );
        assert_eq!(
            room.surrender(first.id, first_player_id).unwrap_err(),
            GameError::InvalidState
        );
        let (post_game_signal, duplicate) = room
            .send_chat(
                second.id,
                Uuid::new_v4(),
                ChatMessageType::QuickCommand,
                None,
                Some(QuickCommandId::GoodGame),
            )
            .unwrap();
        assert!(!duplicate);
        assert_eq!(post_game_signal.content, "굿게임");
    }

    #[test]
    fn lobby_readiness_and_explicit_start_are_idempotent_authoritative_and_race_safe() {
        let host = session("Alpha");
        let guest = session("Bravo");
        let mut room = GameRoom::new(
            "ABC234".to_string(),
            "Test operation".to_string(),
            RoomVisibility::Private,
            &host,
        )
        .unwrap();
        room.join(&guest).unwrap();
        let host_player_id = room.player_for_session(host.id).unwrap().id;
        let guest_player_id = room.player_for_session(guest.id).unwrap().id;
        let host_ready_request = Uuid::new_v4();
        let (accepted, duplicate) = room
            .set_lobby_ready(host.id, host_ready_request, host_player_id, true)
            .unwrap();
        assert!(!duplicate);
        assert_eq!(accepted.player_id, host_player_id);
        assert_eq!(room.status, RoomStatus::WaitingForReady);
        let version_after_ready = room.version;
        let (replayed, duplicate) = room
            .set_lobby_ready(host.id, host_ready_request, host_player_id, true)
            .unwrap();
        assert!(duplicate);
        assert_eq!(replayed, accepted);
        assert_eq!(room.version, version_after_ready);

        room.set_lobby_ready(guest.id, Uuid::new_v4(), guest_player_id, true)
            .unwrap();
        assert_eq!(room.status, RoomStatus::ReadyToStart);
        assert!(room.game.is_none(), "both ready must not auto-start");

        let unready_request = Uuid::new_v4();
        room.set_lobby_ready(guest.id, unready_request, guest_player_id, false)
            .unwrap();
        assert_eq!(room.status, RoomStatus::WaitingForReady);
        let version_after_unready = room.version;
        let (_, duplicate) = room
            .set_lobby_ready(guest.id, unready_request, guest_player_id, false)
            .unwrap();
        assert!(duplicate);
        assert_eq!(room.version, version_after_unready);

        room.set_lobby_ready(guest.id, Uuid::new_v4(), guest_player_id, true)
            .unwrap();
        let ready_version = room.version;
        assert_eq!(
            room.start_placement(guest.id, Uuid::new_v4(), guest_player_id, ready_version,)
                .unwrap_err(),
            GameError::NotHost
        );
        assert_eq!(
            room.start_placement(host.id, Uuid::new_v4(), host_player_id, ready_version - 1,)
                .unwrap_err(),
            GameError::StaleRoomVersion
        );

        room.disconnect(guest.id, 90).unwrap();
        assert_eq!(
            room.start_placement(host.id, Uuid::new_v4(), host_player_id, room.version)
                .unwrap_err(),
            GameError::PlayerDisconnected
        );
        room.reconnect(guest.id).unwrap();

        let start_request = Uuid::new_v4();
        let (started, duplicate) = room
            .start_placement(host.id, start_request, host_player_id, room.version)
            .unwrap();
        assert!(!duplicate);
        assert_eq!(room.status, RoomStatus::Placement);
        let started_version = room.version;
        let (replayed, duplicate) = room
            .start_placement(host.id, start_request, host_player_id, ready_version)
            .unwrap();
        assert!(duplicate);
        assert_eq!(replayed, started);
        assert_eq!(room.version, started_version);
        assert_eq!(
            room.start_placement(host.id, Uuid::new_v4(), host_player_id, room.version)
                .unwrap_err(),
            GameError::GameAlreadyStarted
        );
        assert_eq!(
            room.set_lobby_ready(host.id, Uuid::new_v4(), host_player_id, false)
                .unwrap_err(),
            GameError::GameAlreadyStarted
        );
    }

    #[test]
    fn confirmation_rejects_a_placement_that_differs_from_server_state() {
        let host = session("Alpha");
        let guest = session("Bravo");
        let mut room = GameRoom::new(
            "ABC234".to_string(),
            "Test operation".to_string(),
            RoomVisibility::Private,
            &host,
        )
        .unwrap();
        room.join(&guest).unwrap();
        prepare_placement(&mut room, &host, &guest);
        room.place_ships(host.id, fleet(0)).unwrap();
        assert_eq!(
            room.confirm_placement(host.id, &fleet(5), 60).unwrap_err(),
            GameError::PlacementMismatch
        );
    }

    #[test]
    fn typed_chat_is_validated_idempotent_rate_limited_and_room_scoped() {
        let host = session("Alpha");
        let other_host = session("Charlie");
        let mut room = GameRoom::new(
            "ABC234".to_string(),
            "Test operation".to_string(),
            RoomVisibility::Private,
            &host,
        )
        .unwrap();
        let other_room = GameRoom::new(
            "XYZ234".to_string(),
            "Other operation".to_string(),
            RoomVisibility::Private,
            &other_host,
        )
        .unwrap();
        let now = Utc::now();
        assert_eq!(
            room.send_chat_at(
                other_host.id,
                Uuid::new_v4(),
                ChatMessageType::Text,
                Some("intrusion".to_string()),
                None,
                now,
            )
            .unwrap_err(),
            GameError::NotRoomMember
        );
        assert_eq!(
            room.chat_history(other_host.id).unwrap_err(),
            GameError::NotRoomMember
        );
        let message_id = Uuid::new_v4();
        let (message, duplicate) = room
            .send_chat_at(
                host.id,
                message_id,
                ChatMessageType::Text,
                Some("  ready\nfor battle  ".to_string()),
                None,
                now,
            )
            .unwrap();
        assert!(!duplicate);
        assert_eq!(message.content, "ready\nfor battle");
        assert_eq!(message.room_id, room.id);
        assert_ne!(message.room_id, other_room.id);
        assert_eq!(message.player_id, Some(room.players[0].id));
        assert_eq!(
            room.send_chat_at(
                host.id,
                Uuid::new_v4(),
                ChatMessageType::Text,
                Some("   ".to_string()),
                None,
                now + Duration::seconds(1),
            )
            .unwrap_err(),
            GameError::InvalidChatMessage
        );
        assert_eq!(
            room.send_chat_at(
                host.id,
                Uuid::new_v4(),
                ChatMessageType::Text,
                Some("<script>alert(1)</script>".to_string()),
                None,
                now + Duration::seconds(1),
            )
            .unwrap_err(),
            GameError::InvalidChatMessage
        );
        assert_eq!(
            room.send_chat_at(
                host.id,
                Uuid::new_v4(),
                ChatMessageType::Text,
                Some("x".repeat(301)),
                None,
                now + Duration::seconds(1),
            )
            .unwrap_err(),
            GameError::InvalidChatMessage
        );
        let (emoji, _) = room
            .send_chat_at(
                host.id,
                Uuid::new_v4(),
                ChatMessageType::Emoji,
                Some("🎯".to_string()),
                None,
                now + Duration::seconds(1),
            )
            .unwrap();
        assert_eq!(emoji.message_type, ChatMessageType::Emoji);
        assert_eq!(emoji.content, "🎯");
        assert_eq!(
            room.send_chat_at(
                host.id,
                Uuid::new_v4(),
                ChatMessageType::Emoji,
                Some("<img>".to_string()),
                None,
                now + Duration::seconds(2),
            )
            .unwrap_err(),
            GameError::InvalidEmoji
        );
        let (quick, _) = room
            .send_chat_at(
                host.id,
                Uuid::new_v4(),
                ChatMessageType::QuickCommand,
                None,
                Some(QuickCommandId::NiceShot),
                now + Duration::seconds(3),
            )
            .unwrap();
        assert_eq!(quick.content, "나이스 샷");
        assert_eq!(quick.command_id, Some(QuickCommandId::NiceShot));
        assert_eq!(
            room.send_chat_at(
                host.id,
                Uuid::new_v4(),
                ChatMessageType::QuickCommand,
                None,
                Some(QuickCommandId::NiceShot),
                now + Duration::seconds(4),
            )
            .unwrap_err(),
            GameError::RateLimited
        );

        let before = room.chat_messages.len();
        let (replayed, duplicate) = room
            .send_chat_at(
                host.id,
                message_id,
                ChatMessageType::Text,
                Some("changed".to_string()),
                None,
                now + Duration::seconds(5),
            )
            .unwrap();
        assert!(duplicate);
        assert_eq!(replayed.content, "ready\nfor battle");
        assert_eq!(room.chat_messages.len(), before);

        let spammer = session("Delta");
        let mut spam_room = GameRoom::new(
            "SPM234".to_string(),
            "Spam operation".to_string(),
            RoomVisibility::Private,
            &spammer,
        )
        .unwrap();
        for index in 0..3 {
            spam_room
                .send_chat_at(
                    spammer.id,
                    Uuid::new_v4(),
                    ChatMessageType::Text,
                    Some(format!("message {index}")),
                    None,
                    now,
                )
                .unwrap();
        }
        assert_eq!(
            spam_room
                .send_chat_at(
                    spammer.id,
                    Uuid::new_v4(),
                    ChatMessageType::Emoji,
                    Some("🔥".to_string()),
                    None,
                    now,
                )
                .unwrap_err(),
            GameError::RateLimited
        );

        for index in 0..110 {
            room.push_system_message(format!("system event {index}"));
        }
        assert_eq!(room.chat_messages.len(), MAX_CHAT_HISTORY);
        assert!(
            room.chat_history(host.id)
                .unwrap()
                .iter()
                .all(|entry| entry.room_id == room.id)
        );
    }

    #[test]
    fn turn_expiry_changes_turn_resets_on_attack_and_forfeits_after_three() {
        let (mut room, first, second) = playing_room();
        let timed_out_player = room.game.as_ref().unwrap().current_player_id;
        let timed_out_session = if room.player_for_session(first.id).unwrap().id == timed_out_player
        {
            first.id
        } else {
            second.id
        };
        let opponent_session = if timed_out_session == first.id {
            second.id
        } else {
            first.id
        };

        for cycle in 0..3 {
            let game = room.game.as_ref().unwrap();
            let deadline = game.turn_deadline_at.unwrap();
            let turn = game.turn_number;
            let record = room
                .expire_turn(turn, timed_out_player, deadline, deadline)
                .unwrap()
                .unwrap();
            assert_eq!(record.consecutive_timeout_count, cycle + 1);
            if cycle == 2 {
                assert!(record.winner_id.is_some());
                break;
            }
            let opponent_id = room.player_for_session(opponent_session).unwrap().id;
            let turn = room.game.as_ref().unwrap().turn_number;
            room.fire(
                opponent_session,
                Uuid::new_v4(),
                opponent_id,
                Coordinate {
                    row: 9,
                    col: 9 - cycle,
                },
                room.version,
                turn,
            )
            .unwrap();
        }
        let result = room.game.as_ref().unwrap().result.as_ref().unwrap();
        assert_eq!(result.finish_reason, FinishReason::TurnTimeout);
        assert_eq!(result.win_type, crate::domain::WinType::Timeout);
        assert_eq!(room.status, RoomStatus::Finished);
        assert_eq!(
            result
                .players
                .iter()
                .find(|stats| stats.player_id == timed_out_player)
                .unwrap()
                .total_timeouts,
            3
        );
    }

    #[test]
    fn stale_expiry_cannot_override_a_normal_attack() {
        let (mut room, first, second) = playing_room();
        let current = room.game.as_ref().unwrap().current_player_id;
        let session_id = if room.player_for_session(first.id).unwrap().id == current {
            first.id
        } else {
            second.id
        };
        let old_turn = room.game.as_ref().unwrap().turn_number;
        let old_deadline = room.game.as_ref().unwrap().turn_deadline_at.unwrap();
        room.game.as_mut().unwrap().turn_deadline_at = Some(old_deadline + Duration::seconds(60));
        room.fire(
            session_id,
            Uuid::new_v4(),
            current,
            Coordinate { row: 9, col: 9 },
            room.version,
            old_turn,
        )
        .unwrap();
        assert!(
            room.expire_turn(old_turn, current, old_deadline, old_deadline)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_normal_attack_resets_the_attacking_players_consecutive_timeouts() {
        let (mut room, first, second) = playing_room();
        let timed_out_player = room.game.as_ref().unwrap().current_player_id;
        let timed_out_session = if room.player_for_session(first.id).unwrap().id == timed_out_player
        {
            first.id
        } else {
            second.id
        };
        let opponent_session = if timed_out_session == first.id {
            second.id
        } else {
            first.id
        };

        let game = room.game.as_ref().unwrap();
        let deadline = game.turn_deadline_at.unwrap();
        room.expire_turn(game.turn_number, timed_out_player, deadline, deadline)
            .unwrap()
            .unwrap();

        let opponent_id = room.player_for_session(opponent_session).unwrap().id;
        let turn = room.game.as_ref().unwrap().turn_number;
        room.fire(
            opponent_session,
            Uuid::new_v4(),
            opponent_id,
            Coordinate { row: 9, col: 9 },
            room.version,
            turn,
        )
        .unwrap();

        let turn = room.game.as_ref().unwrap().turn_number;
        room.fire(
            timed_out_session,
            Uuid::new_v4(),
            timed_out_player,
            Coordinate { row: 9, col: 8 },
            room.version,
            turn,
        )
        .unwrap();

        assert_eq!(
            room.game
                .as_ref()
                .unwrap()
                .consecutive_timeout_counts
                .get(&timed_out_player),
            Some(&0)
        );
        assert_eq!(
            room.game
                .as_ref()
                .unwrap()
                .total_timeout_counts
                .get(&timed_out_player),
            Some(&1)
        );
    }

    #[test]
    fn internal_room_state_round_trips_after_attacks() {
        let (mut room, first, second) = playing_room();
        let current_id = room.game.as_ref().unwrap().current_player_id;
        let session_id = if room.player_for_session(first.id).unwrap().id == current_id {
            first.id
        } else {
            second.id
        };
        let version = room.version;
        room.fire(
            session_id,
            Uuid::new_v4(),
            current_id,
            Coordinate { row: 9, col: 9 },
            version,
            1,
        )
        .unwrap();
        let json = serde_json::to_string(&room).unwrap();
        let restored: GameRoom = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.game.unwrap().attacks.len(), 1);
        assert_eq!(restored.chat_messages, room.chat_messages);
    }
}
