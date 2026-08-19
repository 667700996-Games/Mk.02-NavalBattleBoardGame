use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

pub const MAX_CHAT_MESSAGE_CHARS: usize = 300;
pub const MAX_CHAT_HISTORY: usize = 100;
pub const ALLOWED_EMOJIS: [&str; 10] = ["👍", "👏", "😅", "😮", "🔥", "🎯", "🚢", "💥", "🫡", "🤝"];

fn deserialize_quick_command_id<'de, D>(deserializer: D) -> Result<Option<QuickCommandId>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    Ok(value.and_then(|value| QuickCommandId::from_wire(&value)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatMessageType {
    #[serde(rename = "TEXT", alias = "PLAYER")]
    Text,
    #[serde(rename = "QUICK_COMMAND")]
    QuickCommand,
    #[serde(rename = "EMOJI")]
    Emoji,
    #[serde(rename = "SYSTEM")]
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QuickCommandId {
    GoodGame,
    WaitAMoment,
    Ready,
    NiceShot,
    Lucky,
    GoFirst,
    ThankYou,
}

impl QuickCommandId {
    pub const ALL: [Self; 7] = [
        Self::GoodGame,
        Self::WaitAMoment,
        Self::Ready,
        Self::NiceShot,
        Self::Lucky,
        Self::GoFirst,
        Self::ThankYou,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::GoodGame => "굿게임",
            Self::WaitAMoment => "잠시만요",
            Self::Ready => "교전 준비 완료",
            Self::NiceShot => "나이스 샷",
            Self::Lucky => "운이 좋았군요",
            Self::GoFirst => "제가 먼저 가겠습니다",
            Self::ThankYou => "감사합니다",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|command| command.wire_name() == value)
    }

    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::GoodGame => "GOOD_GAME",
            Self::WaitAMoment => "WAIT_A_MOMENT",
            Self::Ready => "READY",
            Self::NiceShot => "NICE_SHOT",
            Self::Lucky => "LUCKY",
            Self::GoFirst => "GO_FIRST",
            Self::ThankYou => "THANK_YOU",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub message_id: Uuid,
    pub room_id: Uuid,
    pub player_id: Option<Uuid>,
    pub nickname: String,
    #[serde(alias = "message")]
    pub content: String,
    pub timestamp: DateTime<Utc>,
    #[serde(rename = "type", alias = "kind")]
    pub message_type: ChatMessageType,
    #[serde(default, deserialize_with = "deserialize_quick_command_id")]
    pub command_id: Option<QuickCommandId>,
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
