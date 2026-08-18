use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SocialRelationship {
    pub target_identity_id: Uuid,
    pub target_nickname: String,
    pub muted: bool,
    pub blocked: bool,
    #[serde(default)]
    pub friend_state: SocialFriendState,
    #[serde(default)]
    pub friend_request_id: Option<Uuid>,
    #[serde(default)]
    pub party_state: SocialPartyState,
    #[serde(default)]
    pub party_id: Option<Uuid>,
    #[serde(default)]
    pub game_invite: Option<DirectGameInvite>,
    #[serde(default)]
    pub presence: SocialPresence,
    #[serde(default)]
    pub current_room_id: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
}

impl SocialRelationship {
    pub fn new(target_identity_id: Uuid, target_nickname: String, now: DateTime<Utc>) -> Self {
        Self {
            target_identity_id,
            target_nickname,
            muted: false,
            blocked: false,
            friend_state: SocialFriendState::None,
            friend_request_id: None,
            party_state: SocialPartyState::None,
            party_id: None,
            game_invite: None,
            presence: SocialPresence::Offline,
            current_room_id: None,
            updated_at: now,
        }
    }

    pub fn has_effect(&self) -> bool {
        self.muted
            || self.blocked
            || self.friend_state != SocialFriendState::None
            || self.party_state != SocialPartyState::None
            || self.game_invite.is_some()
    }

    pub fn clear_social_state(&mut self) {
        self.friend_state = SocialFriendState::None;
        self.friend_request_id = None;
        self.party_state = SocialPartyState::None;
        self.party_id = None;
        self.game_invite = None;
        self.presence = SocialPresence::Offline;
        self.current_room_id = None;
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SocialFriendState {
    #[default]
    None,
    Outgoing,
    Incoming,
    Friend,
}

impl SocialFriendState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Outgoing => "OUTGOING",
            Self::Incoming => "INCOMING",
            Self::Friend => "FRIEND",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "NONE" => Some(Self::None),
            "OUTGOING" => Some(Self::Outgoing),
            "INCOMING" => Some(Self::Incoming),
            "FRIEND" => Some(Self::Friend),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SocialPartyState {
    #[default]
    None,
    OutgoingInvite,
    IncomingInvite,
    Owner,
    Member,
}

impl SocialPartyState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::OutgoingInvite => "OUTGOING_INVITE",
            Self::IncomingInvite => "INCOMING_INVITE",
            Self::Owner => "OWNER",
            Self::Member => "MEMBER",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "NONE" => Some(Self::None),
            "OUTGOING_INVITE" => Some(Self::OutgoingInvite),
            "INCOMING_INVITE" => Some(Self::IncomingInvite),
            "OWNER" => Some(Self::Owner),
            "MEMBER" => Some(Self::Member),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SocialPresence {
    #[default]
    Offline,
    Online,
    InGame,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SocialInviteDirection {
    Outgoing,
    Incoming,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DirectGameInvite {
    pub id: Uuid,
    pub direction: SocialInviteDirection,
    pub room_id: Uuid,
    pub room_code: String,
    pub room_name: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SocialPrivacy {
    pub allow_friend_requests: bool,
    pub show_presence: bool,
    pub allow_game_invites: bool,
    pub updated_at: DateTime<Utc>,
}

impl SocialPrivacy {
    pub fn open(now: DateTime<Utc>) -> Self {
        Self {
            allow_friend_requests: true,
            show_presence: true,
            allow_game_invites: true,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentPlayer {
    pub account_id: Uuid,
    pub handle: String,
    pub last_played_at: DateTime<Utc>,
    #[serde(default)]
    pub friend: bool,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub blocked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SocialOverview {
    pub privacy: SocialPrivacy,
    pub relationships: Vec<SocialRelationship>,
    pub recent_players: Vec<RecentPlayer>,
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
