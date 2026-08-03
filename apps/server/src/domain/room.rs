use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::GameError;

use super::{
    AttackOutcome, AttackRecord, Board, ConnectionState, Coordinate, FinishReason, Game,
    GameResult, Player, ShipKind, ShipPlacement, UserSession,
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
        Ok(Self {
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
        })
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
        let player_id = self.player_for_session(session_id)?.id;
        let opponent_id = self
            .players
            .iter()
            .find(|player| player.id != player_id)
            .map(|player| player.id);
        if self.status == RoomStatus::Playing {
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
        self.bump();
        if record.winner_id.is_some() {
            self.status = RoomStatus::Finished;
            self.bump();
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
        let player_id = self.player_for_session(session_id)?.id;
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
        Ok(deadline)
    }

    pub fn reconnect(&mut self, session_id: Uuid) -> Result<(), GameError> {
        let player_id = self.player_for_session(session_id)?.id;
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
        Ok(())
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
        Ok(accepted)
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
