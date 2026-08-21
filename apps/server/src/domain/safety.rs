use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SafetyRelationship {
    pub target_identity_id: Uuid,
    pub target_nickname: String,
    pub muted: bool,
    pub blocked: bool,
    pub updated_at: DateTime<Utc>,
}

impl SafetyRelationship {
    pub fn new(target_identity_id: Uuid, target_nickname: String, now: DateTime<Utc>) -> Self {
        Self {
            target_identity_id,
            target_nickname,
            muted: false,
            blocked: false,
            updated_at: now,
        }
    }

    pub fn has_effect(&self) -> bool {
        self.muted || self.blocked
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReportCategory {
    Chat,
    Name,
    Cheating,
    Stalling,
    Other,
}

impl ReportCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "CHAT",
            Self::Name => "NAME",
            Self::Cheating => "CHEATING",
            Self::Stalling => "STALLING",
            Self::Other => "OTHER",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "CHAT" => Some(Self::Chat),
            "NAME" => Some(Self::Name),
            "CHEATING" => Some(Self::Cheating),
            "STALLING" => Some(Self::Stalling),
            "OTHER" => Some(Self::Other),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerReportReceipt {
    pub report_id: Uuid,
    pub status: &'static str,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewPlayerReport {
    pub id: Uuid,
    pub reporter_identity_id: Uuid,
    pub target_identity_id: Uuid,
    pub room_id: Uuid,
    pub target_player_id: Uuid,
    pub target_nickname: String,
    pub category: ReportCategory,
    pub details: String,
    pub evidence: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReportStatus {
    Open,
    Reviewing,
    Actioned,
    Dismissed,
}

impl ReportStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "OPEN",
            Self::Reviewing => "REVIEWING",
            Self::Actioned => "ACTIONED",
            Self::Dismissed => "DISMISSED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "OPEN" => Some(Self::Open),
            "REVIEWING" => Some(Self::Reviewing),
            "ACTIONED" => Some(Self::Actioned),
            "DISMISSED" => Some(Self::Dismissed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerReport {
    pub id: Uuid,
    pub reporter_identity_id: Uuid,
    pub target_identity_id: Uuid,
    pub room_id: Uuid,
    pub target_player_id: Uuid,
    pub target_nickname: String,
    pub category: ReportCategory,
    pub details: String,
    pub evidence: serde_json::Value,
    pub status: ReportStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&NewPlayerReport> for PlayerReport {
    fn from(report: &NewPlayerReport) -> Self {
        Self {
            id: report.id,
            reporter_identity_id: report.reporter_identity_id,
            target_identity_id: report.target_identity_id,
            room_id: report.room_id,
            target_player_id: report.target_player_id,
            target_nickname: report.target_nickname.clone(),
            category: report.category,
            details: report.details.clone(),
            evidence: report.evidence.clone(),
            status: ReportStatus::Open,
            created_at: report.created_at,
            updated_at: report.created_at,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModerationActionKind {
    Warn,
    Suspend,
    Ban,
    Dismiss,
    Reverse,
}

impl ModerationActionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warn => "WARN",
            Self::Suspend => "SUSPEND",
            Self::Ban => "BAN",
            Self::Dismiss => "DISMISS",
            Self::Reverse => "REVERSE",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "WARN" => Some(Self::Warn),
            "SUSPEND" => Some(Self::Suspend),
            "BAN" => Some(Self::Ban),
            "DISMISS" => Some(Self::Dismiss),
            "REVERSE" => Some(Self::Reverse),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModerationAction {
    pub id: Uuid,
    pub report_id: Uuid,
    pub target_identity_id: Uuid,
    pub operator_id: String,
    pub action: ModerationActionKind,
    pub reason: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub reverses_action_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewModerationAction {
    pub id: Uuid,
    pub report_id: Uuid,
    pub operator_id: String,
    pub action: ModerationActionKind,
    pub reason: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub reverses_action_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModerationCase {
    pub report: PlayerReport,
    pub actions: Vec<ModerationAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModerationCasePage {
    pub cases: Vec<ModerationCase>,
    pub next_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePenalty {
    Suspended(DateTime<Utc>),
    Banned,
}
