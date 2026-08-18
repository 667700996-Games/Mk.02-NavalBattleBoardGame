use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AccountSession, PlayerAccount};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupportActionKind {
    RevokeSession,
    RevokeAllSessions,
}

#[derive(Debug, Clone)]
pub struct NewSupportAction {
    pub id: Uuid,
    pub account_id: Uuid,
    pub operator_id: String,
    pub action: SupportActionKind,
    pub reason: String,
    pub target_session_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportAction {
    pub id: Uuid,
    pub account_id: Uuid,
    pub operator_id: String,
    pub action: SupportActionKind,
    pub reason: String,
    pub target_session_id: Option<Uuid>,
    pub affected_session_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportAccountSnapshot {
    pub account: PlayerAccount,
    pub sessions: Vec<AccountSession>,
    pub actions: Vec<SupportAction>,
}
