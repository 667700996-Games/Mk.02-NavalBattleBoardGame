use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::GameError;

use super::{
    AttackOutcome, AttackRecord, Board, ChatMessage, ChatMessageKind, ChatTypingEvent,
    ConnectionState, Coordinate, FinishReason, Game, GameResult, MAX_CHAT_HISTORY,
    MAX_CHAT_MESSAGE_CHARS, Player, ShipKind, ShipPlacement, SurrenderRecord, UserSession,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoomStatus {
    Waiting,
    Placement,
    Ready,
    Playing,
    Disconnected,
    Finished,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoomVisibility {
    Public,
    Private,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRoom {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub visibility: RoomVisibility,
    pub status: RoomStatus,
    pub resume_status: Option<RoomStatus>,
    pub players: Vec<Player>,
    pub pending_placements: HashMap<Uuid, Vec<ShipPlacement>>,
    pub game: Option<Game>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub disconnected_deadlines: HashMap<Uuid, DateTime<Utc>>,
    pub rematch_requests: HashSet<Uuid>,
    #[serde(default)]
    pub chat_messages: Vec<ChatMessage>,
    #[serde(skip, default)]
    chat_rate_windows: HashMap<Uuid, Vec<DateTime<Utc>>>,
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
        let mut room = Self {
            id: Uuid::new_v4(),
            code,
            name,
            visibility,
            status: RoomStatus::Waiting,
            resume_status: None,
            players: vec![Player::new(host_session, true)],
            pending_placements: HashMap::new(),
            game: None,
            version: 1,
            created_at: now,
            updated_at: now,
            disconnected_deadlines: HashMap::new(),
            rematch_requests: HashSet::new(),
            chat_messages: Vec::new(),
            chat_rate_windows: HashMap::new(),
        };
        room.push_system_message(format!(
            "{} 지휘관이 작전실에 입장했습니다.",
            host_session.nickname
        ));
        Ok(room)
    }

    pub fn join(&mut self, session: &UserSession) -> Result<Uuid, GameError> {
        if !matches!(self.status, RoomStatus::Waiting) {
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
        self.status = RoomStatus::Placement;
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

    pub fn set_ready(
        &mut self,
        session_id: Uuid,
        claimed_player_id: Uuid,
        ready: bool,
    ) -> Result<(), GameError> {
        if !matches!(self.status, RoomStatus::Waiting | RoomStatus::Placement) {
            return Err(GameError::InvalidState);
        }
        let player_id = self.player_for_session(session_id)?.id;
        if player_id != claimed_player_id {
            return Err(GameError::Unauthorized);
        }
        if let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        {
            player.is_ready = ready;
        }
        self.bump();
        Ok(())
    }

    pub fn leave(&mut self, session_id: Uuid) -> Result<(), GameError> {
        let leaving_player = self.player_for_session(session_id)?.clone();
        let player_id = leaving_player.id;
        let opponent_id = self
            .players
            .iter()
            .find(|player| player.id != player_id)
            .map(|player| player.id);
        let was_active_game = matches!(self.status, RoomStatus::Playing | RoomStatus::Disconnected)
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

    pub fn confirm_placement(&mut self, session_id: Uuid) -> Result<bool, GameError> {
        if self.status != RoomStatus::Placement {
            return Err(GameError::InvalidState);
        }
        let player_id = self.player_for_session(session_id)?.id;
        if !self.pending_placements.contains_key(&player_id) {
            return Err(GameError::IncompleteFleet);
        }
        if let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        {
            player.placement_confirmed = true;
            player.is_ready = true;
        }
        self.bump();

        let all_ready =
            self.players.len() == 2 && self.players.iter().all(|player| player.placement_confirmed);
        if all_ready {
            self.status = RoomStatus::Ready;
            let mut boards = HashMap::new();
            for player in &self.players {
                let placements = self
                    .pending_placements
                    .get(&player.id)
                    .ok_or(GameError::IncompleteFleet)?;
                boards.insert(player.id, Board::from_placements(placements)?);
            }
            self.game = Some(Game::new(boards)?);
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
        if !matches!(
            self.status,
            RoomStatus::Finished | RoomStatus::Cancelled | RoomStatus::Disconnected
        ) {
            self.resume_status = Some(self.status);
            self.status = RoomStatus::Disconnected;
        }
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
        if self.status == RoomStatus::Disconnected
            && self
                .players
                .iter()
                .all(|player| player.connection_state == ConnectionState::Online)
        {
            self.status = self.resume_status.take().unwrap_or(RoomStatus::Waiting);
        }
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
        let disconnected_nickname = self
            .players
            .iter()
            .find(|player| player.id == player_id)
            .map(|player| player.nickname.clone())
            .unwrap_or_else(|| "UNKNOWN".to_string());
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
        if let (Some(game), Some(winner_id)) = (self.game.as_mut(), opponent_id) {
            game.forfeit(winner_id, FinishReason::DisconnectTimeout)?;
            self.status = RoomStatus::Finished;
        } else {
            self.status = RoomStatus::Cancelled;
        }
        self.disconnected_deadlines.remove(&player_id);
        self.resume_status = None;
        self.bump();
        let message = if let Some(winner_id) = opponent_id {
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
            self.pending_placements.clear();
            self.rematch_requests.clear();
            for player in &mut self.players {
                player.placement_confirmed = false;
                player.is_ready = false;
            }
            self.status = RoomStatus::Placement;
        }
        self.bump();
        if accepted {
            self.push_system_message("재대결이 승인되었습니다. 함대 배치를 다시 시작합니다.");
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
        let active = self.status == RoomStatus::Playing
            || (self.status == RoomStatus::Disconnected
                && self.resume_status == Some(RoomStatus::Playing));
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
        self.resume_status = None;
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
        message: String,
    ) -> Result<ChatMessage, GameError> {
        self.send_chat_at(session_id, message, Utc::now())
    }

    fn send_chat_at(
        &mut self,
        session_id: Uuid,
        message: String,
        now: DateTime<Utc>,
    ) -> Result<ChatMessage, GameError> {
        if !matches!(
            self.status,
            RoomStatus::Waiting
                | RoomStatus::Placement
                | RoomStatus::Ready
                | RoomStatus::Playing
                | RoomStatus::Disconnected
        ) {
            return Err(GameError::InvalidState);
        }
        let player = self.player_for_session(session_id)?.clone();
        let normalized = normalize_chat_message(message)?;
        let window = self.chat_rate_windows.entry(player.id).or_default();
        window.retain(|sent_at| now.signed_duration_since(*sent_at).num_seconds() < 5);
        if window.len() >= 5
            || window
                .last()
                .is_some_and(|sent_at| now.signed_duration_since(*sent_at).num_milliseconds() < 400)
        {
            return Err(GameError::RateLimited);
        }
        window.push(now);
        let message = ChatMessage {
            message_id: Uuid::new_v4(),
            room_id: self.id,
            player_id: Some(player.id),
            nickname: player.nickname,
            message: normalized,
            timestamp: now,
            kind: ChatMessageKind::Player,
        };
        self.append_chat_message(message.clone());
        Ok(message)
    }

    pub fn chat_history(&self, session_id: Uuid) -> Result<Vec<ChatMessage>, GameError> {
        self.player_for_session(session_id)?;
        Ok(self.chat_messages.clone())
    }

    pub fn typing_event(
        &self,
        session_id: Uuid,
        is_typing: bool,
    ) -> Result<ChatTypingEvent, GameError> {
        if matches!(self.status, RoomStatus::Finished | RoomStatus::Cancelled) {
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
            message: message.into(),
            timestamp: Utc::now(),
            kind: ChatMessageKind::System,
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
            player_count: self.players.len() as u8,
            capacity: 2,
            created_at: self.created_at,
        }
    }

    pub fn snapshot_for(&self, session_id: Uuid) -> Result<GameSnapshot, GameError> {
        let me = self.player_for_session(session_id)?;
        let players = self.players.iter().map(PlayerPublic::from).collect();
        let (own_board, target_board, turn_number, current_player_id, result) =
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
                (
                    Some(own),
                    Some(target),
                    Some(game.turn_number),
                    Some(game.current_player_id),
                    game.result.clone(),
                )
            } else {
                (None, None, None, None, None)
            };

        Ok(GameSnapshot {
            room: self.summary(),
            version: self.version,
            self_player_id: me.id,
            players,
            own_board,
            target_board,
            turn_number,
            current_player_id,
            result,
            reconnect_deadline: self.disconnected_deadlines.values().min().copied(),
            rematch_requested_by: self.rematch_requests.iter().copied().collect(),
            placement: self.pending_placements.get(&me.id).cloned(),
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
    pub player_count: u8,
    pub capacity: u8,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerPublic {
    pub id: Uuid,
    pub nickname: String,
    pub is_host: bool,
    pub is_ready: bool,
    pub placement_confirmed: bool,
    pub connection_state: ConnectionState,
}

impl From<&Player> for PlayerPublic {
    fn from(player: &Player) -> Self {
        Self {
            id: player.id,
            nickname: player.nickname.clone(),
            is_host: player.is_host,
            is_ready: player.is_ready,
            placement_confirmed: player.placement_confirmed,
            connection_state: player.connection_state,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSnapshot {
    pub room: RoomSummary,
    pub version: u64,
    pub self_player_id: Uuid,
    pub players: Vec<PlayerPublic>,
    pub own_board: Option<OwnBoardSnapshot>,
    pub target_board: Option<TargetBoardSnapshot>,
    pub turn_number: Option<u32>,
    pub current_player_id: Option<Uuid>,
    pub result: Option<GameResult>,
    pub reconnect_deadline: Option<DateTime<Utc>>,
    pub rematch_requested_by: Vec<Uuid>,
    pub placement: Option<Vec<ShipPlacement>>,
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
        room.place_ships(first.id, fleet(0)).unwrap();
        room.place_ships(second.id, fleet(5)).unwrap();
        assert!(!room.confirm_placement(first.id).unwrap());
        assert!(room.confirm_placement(second.id).unwrap());
        (room, first, second)
    }

    #[test]
    fn follows_waiting_placement_playing_state_machine() {
        let host = session("Alpha");
        let guest = session("Bravo");
        let mut room = GameRoom::new(
            "ABC234".to_string(),
            "Test operation".to_string(),
            RoomVisibility::Public,
            &host,
        )
        .unwrap();
        assert_eq!(room.status, RoomStatus::Waiting);
        room.join(&guest).unwrap();
        assert_eq!(room.status, RoomStatus::Placement);
        assert_eq!(
            room.confirm_placement(host.id).unwrap_err(),
            GameError::IncompleteFleet
        );
        room.place_ships(host.id, fleet(0)).unwrap();
        room.place_ships(guest.id, fleet(5)).unwrap();
        assert!(!room.confirm_placement(host.id).unwrap());
        assert!(room.confirm_placement(guest.id).unwrap());
        assert_eq!(room.status, RoomStatus::Playing);
        assert!(room.game.is_some());
        assert_eq!(
            room.place_ships(host.id, fleet(0)).unwrap_err(),
            GameError::InvalidState
        );
    }

    #[test]
    fn personalized_snapshot_never_contains_opponent_ships_or_session_ids() {
        let (room, first, second) = playing_room();
        let first_snapshot = serde_json::to_value(room.snapshot_for(first.id).unwrap()).unwrap();
        let second_snapshot = serde_json::to_value(room.snapshot_for(second.id).unwrap()).unwrap();

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
        assert_eq!(room.status, RoomStatus::Disconnected);
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
                .message
                .contains("Commander Alpha surrendered")
        );
        assert_eq!(
            room.surrender(first.id, first_player_id).unwrap_err(),
            GameError::InvalidState
        );
    }

    #[test]
    fn chat_is_plain_text_rate_limited_room_scoped_and_bounded() {
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
            room.send_chat_at(other_host.id, "intrusion".to_string(), now)
                .unwrap_err(),
            GameError::NotRoomMember
        );
        assert_eq!(
            room.chat_history(other_host.id).unwrap_err(),
            GameError::NotRoomMember
        );
        let message = room
            .send_chat_at(host.id, "  ready\nfor battle  ".to_string(), now)
            .unwrap();
        assert_eq!(message.message, "ready\nfor battle");
        assert_eq!(message.room_id, room.id);
        assert_ne!(message.room_id, other_room.id);
        assert_eq!(message.player_id, Some(room.players[0].id));
        assert_eq!(
            room.send_chat_at(host.id, "   ".to_string(), now + Duration::seconds(1))
                .unwrap_err(),
            GameError::InvalidChatMessage
        );
        assert_eq!(
            room.send_chat_at(
                host.id,
                "<script>alert(1)</script>".to_string(),
                now + Duration::seconds(1)
            )
            .unwrap_err(),
            GameError::InvalidChatMessage
        );
        assert_eq!(
            room.send_chat_at(host.id, "x".repeat(301), now + Duration::seconds(1))
                .unwrap_err(),
            GameError::InvalidChatMessage
        );
        assert_eq!(
            room.send_chat_at(
                host.id,
                "too fast".to_string(),
                now + Duration::milliseconds(100)
            )
            .unwrap_err(),
            GameError::RateLimited
        );
        for second in 1..=4 {
            room.send_chat_at(
                host.id,
                format!("message {second}"),
                now + Duration::seconds(second),
            )
            .unwrap();
        }
        assert_eq!(
            room.send_chat_at(
                host.id,
                "flood".to_string(),
                now + Duration::milliseconds(4500)
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
