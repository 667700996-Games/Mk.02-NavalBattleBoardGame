use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_CHAT_MESSAGE_CHARS: usize = 300;
pub const MAX_CHAT_HISTORY: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChatMessageKind {
    Player,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub message_id: Uuid,
    pub room_id: Uuid,
    pub player_id: Option<Uuid>,
    pub nickname: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub kind: ChatMessageKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTypingEvent {
    pub room_id: Uuid,
    pub player_id: Uuid,
    pub nickname: String,
    pub is_typing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurrenderRecord {
    pub room_id: Uuid,
    pub surrendered_player_id: Uuid,
    pub winner_id: Uuid,
    pub nickname: String,
    pub timestamp: DateTime<Utc>,
}
