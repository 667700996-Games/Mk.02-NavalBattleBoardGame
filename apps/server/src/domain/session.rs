use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct UserSession {
    pub id: Uuid,
    pub account_id: Option<Uuid>,
    pub nickname: String,
    pub token_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub current_room_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerAccount {
    pub id: Uuid,
    pub handle: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSession {
    pub id: Uuid,
    pub nickname: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub current_room_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConnectionState {
    Online,
    Reconnecting,
    Offline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlayerReadyState {
    #[default]
    NotReady,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlayerRole {
    Host,
    #[default]
    Guest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlayerKind {
    #[default]
    Human,
    Ai,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub id: Uuid,
    pub session_id: Uuid,
    pub nickname: String,
    #[serde(default)]
    pub kind: PlayerKind,
    #[serde(default)]
    pub role: PlayerRole,
    #[serde(default)]
    pub is_host: bool,
    pub placement_confirmed: bool,
    #[serde(default)]
    pub ready_state: PlayerReadyState,
    pub connection_state: ConnectionState,
    #[serde(default = "Utc::now")]
    pub joined_at: DateTime<Utc>,
    #[serde(default)]
    pub ready_at: Option<DateTime<Utc>>,
}

impl Player {
    pub fn new(session: &UserSession, is_host: bool) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            session_id: session.id,
            nickname: session.nickname.clone(),
            kind: PlayerKind::Human,
            role: if is_host {
                PlayerRole::Host
            } else {
                PlayerRole::Guest
            },
            is_host,
            placement_confirmed: false,
            ready_state: PlayerReadyState::NotReady,
            connection_state: ConnectionState::Online,
            joined_at: now,
            ready_at: None,
        }
    }
}
