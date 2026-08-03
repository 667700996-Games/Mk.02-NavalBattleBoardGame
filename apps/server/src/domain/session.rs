use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct UserSession {
    pub id: Uuid,
    pub nickname: String,
    pub token_hash: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub id: Uuid,
    pub session_id: Uuid,
    pub nickname: String,
    pub is_host: bool,
    pub is_ready: bool,
    pub placement_confirmed: bool,
    pub connection_state: ConnectionState,
}

impl Player {
    pub fn new(session: &UserSession, is_host: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id: session.id,
            nickname: session.nickname.clone(),
            is_host,
            is_ready: false,
            placement_confirmed: false,
            connection_state: ConnectionState::Online,
        }
    }
}

