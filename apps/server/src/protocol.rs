use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{
    AttackRecord, ChatMessage, ChatMessageType, ChatTypingEvent, Coordinate, GameSnapshot,
    GameTimerState, RoomSummary, RoomVisibility, ShipPlacement, SurrenderRecord, TurnExpiredRecord,
    UnreadyRecord,
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
    pub placements: Vec<ShipPlacement>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnreadyInput {
    pub request_id: Uuid,
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
pub struct SurrenderInput {
    pub room_id: Uuid,
    pub player_id: Uuid,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatSendInput {
    pub room_id: Uuid,
    pub client_message_id: Uuid,
    #[serde(rename = "type")]
    pub message_type: ChatMessageType,
    pub content: Option<String>,
    pub command_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatTypingInput {
    pub room_id: Uuid,
    pub is_typing: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryResponse {
    pub room_id: Uuid,
    pub messages: Vec<ChatMessage>,
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
    #[serde(rename = "player:unready")]
    PlayerUnready(UnreadyInput),
    #[serde(rename = "attack:fire")]
    AttackFire(AttackFireInput),
    #[serde(rename = "game:surrender")]
    GameSurrender(SurrenderInput),
    #[serde(rename = "chat:send")]
    ChatSend(ChatSendInput),
    #[serde(rename = "chat:typing")]
    ChatTyping(ChatTypingInput),
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
    #[serde(rename = "player:unready:accepted")]
    PlayerUnreadyAccepted(UnreadyRecord),
    #[serde(rename = "player:unready:rejected")]
    PlayerUnreadyRejected(ProtocolError),
    #[serde(rename = "game:started")]
    GameStarted(GameSnapshot),
    #[serde(rename = "turn:changed")]
    TurnChanged(GameSnapshot),
    #[serde(rename = "turn:started")]
    TurnStarted(GameTimerState),
    #[serde(rename = "turn:expired")]
    TurnExpired(TurnExpiredRecord),
    #[serde(rename = "game:timer-sync")]
    GameTimerSync(GameTimerState),
    #[serde(rename = "attack:result")]
    AttackResult(AttackRecord),
    #[serde(rename = "ship:sunk")]
    ShipSunk(AttackRecord),
    #[serde(rename = "game:finished")]
    GameFinished(GameSnapshot),
    #[serde(rename = "game:surrendered")]
    GameSurrendered(SurrenderRecord),
    #[serde(rename = "chat:message")]
    ChatMessage(ChatMessage),
    #[serde(rename = "chat:history")]
    ChatHistory(ChatHistoryResponse),
    #[serde(rename = "chat:rejected")]
    ChatRejected(ProtocolError),
    #[serde(rename = "chat:typing")]
    ChatTyping(ChatTypingEvent),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ChatMessageType;

    #[test]
    fn chat_and_surrender_contracts_use_the_public_camel_case_envelope() {
        let room_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let surrender: ClientEvent = serde_json::from_value(serde_json::json!({
            "type": "game:surrender",
            "payload": { "roomId": room_id, "playerId": player_id }
        }))
        .unwrap();
        assert!(matches!(
            surrender,
            ClientEvent::GameSurrender(SurrenderInput {
                room_id: parsed_room,
                player_id: parsed_player
            }) if parsed_room == room_id && parsed_player == player_id
        ));

        let client_message_id = Uuid::new_v4();
        let chat: ClientEvent = serde_json::from_value(serde_json::json!({
            "type": "chat:send",
            "payload": {
                "roomId": room_id,
                "clientMessageId": client_message_id,
                "type": "QUICK_COMMAND",
                "content": null,
                "commandId": "NICE_SHOT"
            }
        }))
        .unwrap();
        assert!(matches!(
            chat,
            ClientEvent::ChatSend(ChatSendInput {
                room_id: parsed_room,
                client_message_id: parsed_message,
                message_type: ChatMessageType::QuickCommand,
                command_id: Some(command_id),
                ..
            }) if parsed_room == room_id && parsed_message == client_message_id && command_id == "NICE_SHOT"
        ));

        assert!(matches!(
            serde_json::from_value::<ClientEvent>(serde_json::json!({
                "type": "chat:send",
                "payload": {
                    "roomId": room_id,
                    "clientMessageId": Uuid::new_v4(),
                    "type": "QUICK_COMMAND",
                    "content": null,
                    "commandId": "ROOT_ACCESS"
                }
            }))
            .unwrap(),
            ClientEvent::ChatSend(ChatSendInput { command_id: Some(command_id), .. })
                if command_id == "ROOT_ACCESS"
        ));

        let timestamp = Utc::now();
        let event = ServerEvent::ChatMessage(ChatMessage {
            message_id: Uuid::new_v4(),
            room_id,
            player_id: Some(player_id),
            nickname: "Alpha".to_string(),
            content: "Sector C4".to_string(),
            timestamp,
            message_type: ChatMessageType::Text,
            command_id: None,
        });
        let serialized = serde_json::to_value(event).unwrap();
        assert_eq!(serialized["type"], "chat:message");
        assert_eq!(serialized["payload"]["roomId"], room_id.to_string());
        assert_eq!(serialized["payload"]["playerId"], player_id.to_string());
        assert_eq!(serialized["payload"]["type"], "TEXT");
        assert!(serialized["payload"].get("message_id").is_none());
    }
}
