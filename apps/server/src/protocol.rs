use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{
    AttackRecord, Coordinate, GameSnapshot, RoomSummary, RoomVisibility, ShipPlacement,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateSessionInput {
    pub nickname: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionResponse {
    pub id: Uuid,
    pub nickname: String,
    pub current_room_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateRoomInput {
    pub name: String,
    pub visibility: RoomVisibility,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinRoomInput {
    pub code: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomCreatedResponse {
    pub snapshot: GameSnapshot,
    pub invite_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomListResponse {
    pub rooms: Vec<RoomSummary>,
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub status: &'static str,
    pub storage: &'static str,
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoomReference {
    pub room_id: Uuid,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadyInput {
    pub room_id: Uuid,
    pub player_id: Uuid,
    pub ready: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaceShipsInput {
    pub room_id: Uuid,
    pub player_id: Uuid,
    pub placements: Vec<ShipPlacement>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmShipsInput {
    pub room_id: Uuid,
    pub player_id: Uuid,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttackFireInput {
    pub request_id: Uuid,
    pub room_id: Uuid,
    pub player_id: Uuid,
    pub coordinate: Coordinate,
    pub expected_version: u64,
    pub turn_number: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeartbeatInput {
    pub client_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ClientEvent {
    #[serde(rename = "room:create")]
    RoomCreate(CreateRoomInput),
    #[serde(rename = "room:join")]
    RoomJoin(JoinRoomInput),
    #[serde(rename = "room:leave")]
    RoomLeave(RoomReference),
    #[serde(rename = "player:ready")]
    PlayerReady(ReadyInput),
    #[serde(rename = "ships:place")]
    ShipsPlace(PlaceShipsInput),
    #[serde(rename = "ships:confirm")]
    ShipsConfirm(ConfirmShipsInput),
    #[serde(rename = "attack:fire")]
    AttackFire(AttackFireInput),
    #[serde(rename = "game:rematch")]
    GameRematch(RoomReference),
    #[serde(rename = "game:sync")]
    GameSync(RoomReference),
    #[serde(rename = "heartbeat")]
    Heartbeat(HeartbeatInput),
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "payload")]
pub enum ServerEvent {
    #[serde(rename = "room:created")]
    RoomCreated(RoomCreatedResponse),
    #[serde(rename = "room:updated")]
    RoomUpdated(GameSnapshot),
    #[serde(rename = "player:joined")]
    PlayerJoined(GameSnapshot),
    #[serde(rename = "player:left")]
    PlayerLeft(GameSnapshot),
    #[serde(rename = "placement:accepted")]
    PlacementAccepted(GameSnapshot),
    #[serde(rename = "placement:rejected")]
    PlacementRejected(ProtocolError),
    #[serde(rename = "game:started")]
    GameStarted(GameSnapshot),
    #[serde(rename = "turn:changed")]
    TurnChanged(GameSnapshot),
    #[serde(rename = "attack:result")]
    AttackResult(AttackRecord),
    #[serde(rename = "ship:sunk")]
    ShipSunk(AttackRecord),
    #[serde(rename = "game:finished")]
    GameFinished(GameSnapshot),
    #[serde(rename = "player:disconnected")]
    PlayerDisconnected(GameSnapshot),
    #[serde(rename = "player:reconnected")]
    PlayerReconnected(GameSnapshot),
    #[serde(rename = "game:snapshot")]
    GameSnapshot(GameSnapshot),
    #[serde(rename = "matchmaking:queued")]
    MatchmakingQueued(MatchmakingStatus),
    #[serde(rename = "matchmaking:cancelled")]
    MatchmakingCancelled(MatchmakingStatus),
    #[serde(rename = "heartbeat")]
    Heartbeat(HeartbeatResponse),
    #[serde(rename = "error")]
    Error(ProtocolError),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub request_id: Uuid,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatResponse {
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakingStatus {
    pub queued: bool,
    pub queued_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakingResponse {
    pub queued: bool,
    pub queued_at: Option<DateTime<Utc>>,
    pub snapshot: Option<GameSnapshot>,
}
