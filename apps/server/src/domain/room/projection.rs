use super::*;

impl GameRoom {
    pub fn summary(&self) -> RoomSummary {
        RoomSummary {
            id: self.id,
            code: self.code.clone(),
            name: self.name.clone(),
            status: self.status,
            rules: self.rules,
            host_player_id: self.host_player_id,
            game_id: self.game_id,
            version: self.version,
            player_count: self.players.len() as u8,
            capacity: 2,
            created_at: self.created_at,
        }
    }

    pub fn snapshot_for(&self, session_id: Uuid) -> Result<GameSnapshot, GameError> {
        self.require_valid_balance()?;
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
                let attacks = if game.timeline.is_empty() {
                    game.attacks
                        .iter()
                        .filter(|attack| attack.attacker_id == me.id)
                        .map(|attack| TargetAttackSnapshot {
                            coordinate: attack.coordinate,
                            outcome: attack.outcome,
                            sunk_ship: attack.sunk_ship,
                        })
                        .collect()
                } else {
                    game.timeline
                        .iter()
                        .flat_map(|event| match event {
                            GameTimelineEvent::Attack(attack) if attack.attacker_id == me.id => {
                                vec![TargetAttackSnapshot {
                                    coordinate: attack.coordinate,
                                    outcome: attack.outcome,
                                    sunk_ship: attack.sunk_ship,
                                }]
                            }
                            GameTimelineEvent::SkillAttack(record)
                                if record.attacker_id == me.id =>
                            {
                                record
                                    .cells
                                    .iter()
                                    .map(|cell| TargetAttackSnapshot {
                                        coordinate: cell.coordinate,
                                        outcome: cell.outcome,
                                        sunk_ship: cell.sunk_ship,
                                    })
                                    .collect()
                            }
                            _ => Vec::new(),
                        })
                        .collect()
                };
                let target = TargetBoardSnapshot { attacks };
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
            balance: self.balance.clone(),
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
            matchmaking_quality: self.matchmaking_quality,
            ranked_match: self.ranked_match.clone(),
            rules: self.rules,
            own_board,
            target_board,
            revealed_board,
            turn_number,
            current_player_id,
            result,
            reconnect_deadline: self.disconnected_deadlines.values().min().copied(),
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
            shots_remaining_in_turn: self.game.as_ref().map(|game| game.shots_remaining_in_turn),
            skill_inventories: self
                .game
                .as_ref()
                .map(|game| game.skill_inventories.clone())
                .unwrap_or_default(),
            skill_used_this_turn: self
                .game
                .as_ref()
                .is_some_and(|game| game.skill_used_turns.get(&me.id) == Some(&game.turn_number)),
            skill_unlock_turn: self
                .balance
                .manifest
                .tactical_skills
                .as_ref()
                .map(|rules| rules.unlock_turn),
            server_timestamp: Utc::now(),
        })
    }

    pub fn replay_for(&self, session_id: Uuid) -> Result<GameReplay, GameError> {
        self.require_valid_balance()?;
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
        let first_player_id = if game.first_player_id.is_nil() {
            timeline
                .first()
                .map(|event| match event {
                    GameTimelineEvent::Attack(record) => record.attacker_id,
                    GameTimelineEvent::SkillAttack(record) => record.attacker_id,
                    GameTimelineEvent::TurnExpired(record) => record.expired_player_id,
                })
                .unwrap_or(game.current_player_id)
        } else {
            game.first_player_id
        };
        Ok(GameReplay {
            protocol_version: crate::PROTOCOL_VERSION,
            ruleset_version: self.balance.ruleset_version,
            balance: self.balance.clone(),
            room_id: self.id,
            room_name: self.name.clone(),
            game_id: self.game_id.ok_or(GameError::InvalidState)?,
            first_player_id,
            started_at: game.started_at,
            finished_at: result.finished_at,
            players,
            timeline,
            result,
        })
    }

    pub fn spectator_snapshot_at(
        &self,
        now: DateTime<Utc>,
    ) -> Result<SpectatorSnapshot, GameError> {
        self.require_valid_balance()?;
        if self.visibility != RoomVisibility::Public {
            return Err(GameError::RoomNotFound);
        }
        if self.status != RoomStatus::Playing {
            return Err(GameError::RoomNotFound);
        }
        let game = self.game.as_ref().ok_or(GameError::InvalidState)?;
        let visible_through = now - Duration::seconds(i64::from(SPECTATOR_DELAY_SECONDS));
        let source_timeline = if game.timeline.is_empty() {
            game.attacks
                .iter()
                .cloned()
                .map(GameTimelineEvent::Attack)
                .collect::<Vec<_>>()
        } else {
            game.timeline.clone()
        };
        let timeline = source_timeline
            .into_iter()
            .filter(|event| match event {
                GameTimelineEvent::Attack(record) => record.created_at <= visible_through,
                GameTimelineEvent::SkillAttack(record) => record.created_at <= visible_through,
                GameTimelineEvent::TurnExpired(record) => record.expired_at <= visible_through,
            })
            .collect::<Vec<_>>();
        let visible_result = game
            .result
            .clone()
            .filter(|result| result.finished_at <= visible_through);
        let phase = if game.started_at > visible_through {
            SpectatorPhase::Delayed
        } else if visible_result.is_some() {
            SpectatorPhase::Finished
        } else {
            SpectatorPhase::Live
        };
        let current_player_id = if phase == SpectatorPhase::Live {
            timeline
                .last()
                .and_then(|event| match event {
                    GameTimelineEvent::Attack(record) => record.next_player_id,
                    GameTimelineEvent::SkillAttack(record) => record.next_player_id,
                    GameTimelineEvent::TurnExpired(record) => record.next_player_id,
                })
                .or_else(|| (!game.first_player_id.is_nil()).then_some(game.first_player_id))
        } else {
            None
        };
        let mut room = self.summary();
        room.status = match phase {
            SpectatorPhase::Delayed => RoomStatus::Placement,
            SpectatorPhase::Live => RoomStatus::Playing,
            SpectatorPhase::Finished => RoomStatus::Finished,
        };

        Ok(SpectatorSnapshot {
            protocol_version: crate::PROTOCOL_VERSION,
            delay_seconds: SPECTATOR_DELAY_SECONDS,
            visible_through,
            room,
            game_id: self.game_id.ok_or(GameError::InvalidState)?,
            phase,
            players: self
                .players
                .iter()
                .map(|player| SpectatorPlayer {
                    id: player.id,
                    nickname: player.nickname.clone(),
                    kind: player.kind,
                })
                .collect(),
            balance: self.balance.clone(),
            rules: self.rules,
            timeline,
            current_player_id,
            result: visible_result,
            server_timestamp: now,
        })
    }

    pub fn has_valid_balance_pin(&self) -> bool {
        self.balance.has_valid_integrity()
            && self
                .game
                .as_ref()
                .is_none_or(|game| game.balance == self.balance)
            && (matches!(self.status, RoomStatus::Finished | RoomStatus::Cancelled)
                || self.balance.is_registered_for_execution())
    }

    pub(super) fn require_valid_balance(&self) -> Result<(), GameError> {
        self.has_valid_balance_pin()
            .then_some(())
            .ok_or(GameError::InvalidState)
    }

    pub(super) fn require_executable_balance(&self) -> Result<(), GameError> {
        (self.has_valid_balance_pin() && self.balance.is_registered_for_execution())
            .then_some(())
            .ok_or(GameError::InvalidState)
    }
}

pub const SPECTATOR_DELAY_SECONDS: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpectatorPhase {
    Delayed,
    Live,
    Finished,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpectatorPlayer {
    pub id: Uuid,
    pub nickname: String,
    pub kind: PlayerKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpectatorSnapshot {
    pub protocol_version: u16,
    pub delay_seconds: u32,
    pub visible_through: DateTime<Utc>,
    pub room: RoomSummary,
    pub game_id: Uuid,
    pub phase: SpectatorPhase,
    pub players: Vec<SpectatorPlayer>,
    pub balance: BalancePin,
    pub rules: MatchRules,
    pub timeline: Vec<GameTimelineEvent>,
    pub current_player_id: Option<Uuid>,
    pub result: Option<GameResult>,
    pub server_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomSummary {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub status: RoomStatus,
    pub rules: MatchRules,
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
    pub balance: BalancePin,
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
    pub matchmaking_quality: Option<MatchmakingQuality>,
    #[serde(default)]
    pub ranked_match: Option<RankedMatchContext>,
    pub rules: MatchRules,
    pub own_board: Option<OwnBoardSnapshot>,
    pub target_board: Option<TargetBoardSnapshot>,
    pub revealed_board: Option<OwnBoardSnapshot>,
    pub turn_number: Option<u32>,
    pub current_player_id: Option<Uuid>,
    pub result: Option<GameResult>,
    pub reconnect_deadline: Option<DateTime<Utc>>,
    pub placement: Option<Vec<ShipPlacement>>,
    pub placement_started_at: Option<DateTime<Utc>>,
    pub game_started_at: Option<DateTime<Utc>>,
    pub game_finished_at: Option<DateTime<Utc>>,
    pub turn_started_at: Option<DateTime<Utc>>,
    pub turn_deadline_at: Option<DateTime<Utc>>,
    pub turn_duration_seconds: Option<u32>,
    pub shots_remaining_in_turn: Option<u8>,
    #[serde(default)]
    pub skill_inventories: HashMap<Uuid, TacticalSkillInventory>,
    #[serde(default)]
    pub skill_used_this_turn: bool,
    pub skill_unlock_turn: Option<u32>,
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
    pub balance: BalancePin,
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
