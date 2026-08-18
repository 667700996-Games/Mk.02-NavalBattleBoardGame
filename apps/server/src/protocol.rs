use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{
    AccountSession, AiDifficulty, AttackRecord, ChatMessage, ChatMessageType, ChatTypingEvent,
    Coordinate, GameSnapshot, GameStartRecord, GameTimerState, IntegritySignalKind,
    LiveContentPayload, LiveContentRevision, MatchRules, MatchmakingPool, MatchmakingQuality,
    MatchmakingRegion, MatchmakingSearchWindow, ModerationAction, ModerationActionKind,
    PlayerAccount, PlayerReadyRecord, PlayerReportReceipt, RankedLeaderboardPage, ReportCategory,
    ReportStatus, RoomSummary, RoomVisibility, ShipPlacement, SocialRelationship, SurrenderRecord,
    TurnExpiredRecord,
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
    pub account_id: Option<Uuid>,
    pub nickname: String,
    pub current_room_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountUpgradeInput {
    pub handle: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountLoginInput {
    pub account_id: Uuid,
    pub recovery_key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUpgradeResponse {
    pub account: PlayerAccount,
    pub recovery_key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSessionsResponse {
    pub current_session_id: Uuid,
    pub sessions: Vec<AccountSession>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountDeletionInput {
    pub recovery_key: String,
    pub confirmation: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDeletionResponse {
    pub request_id: Uuid,
    pub deleted_at: DateTime<Utc>,
    pub stats: crate::store::AccountDeletionStats,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankedLeaderboardQuery {
    pub season_id: Option<String>,
    pub cursor: Option<Uuid>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedLeaderboardResponse {
    #[serde(flatten)]
    pub page: RankedLeaderboardPage,
    pub viewer_visible: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankedLeaderboardVisibilityInput {
    pub visible: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedLeaderboardVisibilityResponse {
    pub visible: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SocialRelationshipInput {
    pub room_id: Uuid,
    pub target_player_id: Uuid,
    pub muted: bool,
    pub blocked: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialRelationshipsResponse {
    pub relationships: Vec<SocialRelationship>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayerReportInput {
    pub room_id: Uuid,
    pub target_player_id: Uuid,
    pub category: ReportCategory,
    pub details: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerReportResponse {
    pub report: PlayerReportReceipt,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModerationReportQuery {
    pub status: Option<ReportStatus>,
    pub search: Option<String>,
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModerationActionInput {
    pub action: ModerationActionKind,
    pub reason: String,
    pub duration_hours: Option<u32>,
    pub reverses_action_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModerationActionResponse {
    pub action: ModerationAction,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishLiveContentInput {
    pub expected_revision: u64,
    pub payload: LiveContentPayload,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RollbackLiveContentInput {
    pub expected_revision: u64,
    pub target_revision: u64,
    pub change_note: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveContentHistoryQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveContentHistoryResponse {
    pub current_revision: u64,
    pub revisions: Vec<LiveContentRevision>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntegritySignalQuery {
    pub kind: Option<IntegritySignalKind>,
    pub search: Option<String>,
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateRoomInput {
    pub name: String,
    pub visibility: RoomVisibility,
    #[serde(default)]
    pub rules: Option<MatchRules>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinRoomInput {
    pub code: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatePracticeInput {
    pub difficulty: AiDifficulty,
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
    pub protocol_version: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub status: &'static str,
    pub storage: &'static str,
    pub server_time: DateTime<Utc>,
    pub protocol_version: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunnelStage {
    Landing,
    TutorialStarted,
    TutorialCompleted,
    SessionCreated,
    LobbyEntered,
    RoomJoined,
    PlacementCompleted,
    FirstAttack,
    MatchCompleted,
}

impl FunnelStage {
    pub const ALL: [Self; 9] = [
        Self::Landing,
        Self::TutorialStarted,
        Self::TutorialCompleted,
        Self::SessionCreated,
        Self::LobbyEntered,
        Self::RoomJoined,
        Self::PlacementCompleted,
        Self::FirstAttack,
        Self::MatchCompleted,
    ];
    pub const COUNT: usize = Self::ALL.len();

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Landing => "landing",
            Self::TutorialStarted => "tutorial_started",
            Self::TutorialCompleted => "tutorial_completed",
            Self::SessionCreated => "session_created",
            Self::LobbyEntered => "lobby_entered",
            Self::RoomJoined => "room_joined",
            Self::PlacementCompleted => "placement_completed",
            Self::FirstAttack => "first_attack",
            Self::MatchCompleted => "match_completed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunnelOutcome {
    Reached,
    Failed,
    Abandoned,
}

impl FunnelOutcome {
    pub const ALL: [Self; 3] = [Self::Reached, Self::Failed, Self::Abandoned];
    pub const COUNT: usize = Self::ALL.len();

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Reached => "reached",
            Self::Failed => "failed",
            Self::Abandoned => "abandoned",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunnelFailureReason {
    Network,
    SessionCreation,
    Authentication,
    RoomEntry,
    Matchmaking,
    Recovery,
    Placement,
    Attack,
}

impl FunnelFailureReason {
    pub const ALL: [Self; 8] = [
        Self::Network,
        Self::SessionCreation,
        Self::Authentication,
        Self::RoomEntry,
        Self::Matchmaking,
        Self::Recovery,
        Self::Placement,
        Self::Attack,
    ];
    pub const COUNT: usize = Self::ALL.len();

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::SessionCreation => "session_creation",
            Self::Authentication => "authentication",
            Self::RoomEntry => "room_entry",
            Self::Matchmaking => "matchmaking",
            Self::Recovery => "recovery",
            Self::Placement => "placement",
            Self::Attack => "attack",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FunnelEventInput {
    pub stage: FunnelStage,
    pub outcome: FunnelOutcome,
    pub reason: Option<FunnelFailureReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RumMetric {
    Lcp,
    Cls,
    Inp,
    BattleInteraction,
}

impl RumMetric {
    pub const ALL: [Self; 4] = [Self::Lcp, Self::Cls, Self::Inp, Self::BattleInteraction];
    pub const COUNT: usize = Self::ALL.len();

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn prometheus_name(self) -> &'static str {
        match self {
            Self::Lcp => "mk01_rum_lcp_milliseconds",
            Self::Cls => "mk01_rum_cls_milli",
            Self::Inp => "mk01_rum_inp_milliseconds",
            Self::BattleInteraction => "mk01_rum_battle_interaction_milliseconds",
        }
    }

    pub const fn help(self) -> &'static str {
        match self {
            Self::Lcp => "Real-user Largest Contentful Paint in milliseconds.",
            Self::Cls => "Real-user Cumulative Layout Shift multiplied by 1000.",
            Self::Inp => "Real-user Interaction to Next Paint in milliseconds.",
            Self::BattleInteraction => {
                "Real-user attack command to authoritative result latency in milliseconds."
            }
        }
    }

    pub const fn buckets(self) -> [u64; 5] {
        match self {
            Self::Lcp => [1_000, 2_500, 4_000, 8_000, 16_000],
            Self::Cls => [50, 100, 250, 500, 1_000],
            Self::Inp | Self::BattleInteraction => [100, 200, 500, 1_000, 2_500],
        }
    }

    pub const fn maximum(self) -> u32 {
        match self {
            Self::Lcp | Self::BattleInteraction => 60_000,
            Self::Cls => 5_000,
            Self::Inp => 30_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RumRoute {
    Landing,
    Tutorial,
    Lobby,
    Join,
    Room,
    Account,
    Replay,
    Other,
}

impl RumRoute {
    pub const ALL: [Self; 8] = [
        Self::Landing,
        Self::Tutorial,
        Self::Lobby,
        Self::Join,
        Self::Room,
        Self::Account,
        Self::Replay,
        Self::Other,
    ];
    pub const COUNT: usize = Self::ALL.len();

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Landing => "landing",
            Self::Tutorial => "tutorial",
            Self::Lobby => "lobby",
            Self::Join => "join",
            Self::Room => "room",
            Self::Account => "account",
            Self::Replay => "replay",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RumDeviceTier {
    Desktop,
    Mobile,
    LowMobile,
}

impl RumDeviceTier {
    pub const ALL: [Self; 3] = [Self::Desktop, Self::Mobile, Self::LowMobile];
    pub const COUNT: usize = Self::ALL.len();

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Mobile => "mobile",
            Self::LowMobile => "low_mobile",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RumMetricInput {
    pub metric: RumMetric,
    pub route: RumRoute,
    pub device_tier: RumDeviceTier,
    pub value: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoomReference {
    pub room_id: Uuid,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadyInput {
    pub request_id: Uuid,
    pub room_id: Uuid,
    pub player_id: Uuid,
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
pub struct GameStartInput {
    pub request_id: Uuid,
    pub room_id: Uuid,
    pub player_id: Uuid,
    pub room_version: u64,
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
    #[serde(rename = "game:start")]
    GameStart(GameStartInput),
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
    #[serde(rename = "player:ready:accepted")]
    PlayerReadyAccepted(PlayerReadyRecord),
    #[serde(rename = "player:ready:rejected")]
    PlayerReadyRejected(ProtocolError),
    #[serde(rename = "player:unready:accepted")]
    PlayerUnreadyAccepted(PlayerReadyRecord),
    #[serde(rename = "player:unready:rejected")]
    PlayerUnreadyRejected(ProtocolError),
    #[serde(rename = "game:start:accepted")]
    GameStartAccepted(GameStartRecord),
    #[serde(rename = "game:start:rejected")]
    GameStartRejected(ProtocolError),
    #[serde(rename = "game:placement-started")]
    GamePlacementStarted(GameSnapshot),
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
    pub ticket: Option<MatchmakingTicket>,
    pub match_quality: Option<MatchmakingQuality>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakingResponse {
    pub queued: bool,
    pub queued_at: Option<DateTime<Utc>>,
    pub ticket: MatchmakingTicket,
    pub match_quality: Option<MatchmakingQuality>,
    pub snapshot: Option<GameSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakingTicket {
    pub pool: MatchmakingPool,
    pub region: MatchmakingRegion,
    pub reported_latency_ms: u16,
    pub rating: Option<i32>,
    pub party_size: u8,
    pub search_window: MatchmakingSearchWindow,
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

    #[test]
    fn lobby_readiness_and_game_start_contracts_require_request_and_room_versions() {
        let room_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let ready_request = Uuid::new_v4();
        let ready: ClientEvent = serde_json::from_value(serde_json::json!({
            "type": "player:ready",
            "payload": {
                "requestId": ready_request,
                "roomId": room_id,
                "playerId": player_id
            }
        }))
        .unwrap();
        assert!(matches!(
            ready,
            ClientEvent::PlayerReady(ReadyInput { request_id, room_id: parsed_room, player_id: parsed_player })
                if request_id == ready_request && parsed_room == room_id && parsed_player == player_id
        ));

        let start_request = Uuid::new_v4();
        let start: ClientEvent = serde_json::from_value(serde_json::json!({
            "type": "game:start",
            "payload": {
                "requestId": start_request,
                "roomId": room_id,
                "playerId": player_id,
                "roomVersion": 17
            }
        }))
        .unwrap();
        assert!(matches!(
            start,
            ClientEvent::GameStart(GameStartInput {
                request_id,
                room_id: parsed_room,
                player_id: parsed_player,
                room_version: 17
            }) if request_id == start_request && parsed_room == room_id && parsed_player == player_id
        ));

        assert!(
            serde_json::from_value::<ClientEvent>(serde_json::json!({
                "type": "game:start",
                "payload": {
                    "requestId": start_request,
                    "roomId": room_id,
                    "playerId": player_id
                }
            }))
            .is_err()
        );
    }
}
