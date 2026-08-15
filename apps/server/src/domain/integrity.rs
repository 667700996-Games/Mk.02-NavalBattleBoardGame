use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegritySignalKind {
    ImpossibleOrder,
    Automation,
    Collusion,
    IntentionalStalling,
}

impl IntegritySignalKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ImpossibleOrder => "IMPOSSIBLE_ORDER",
            Self::Automation => "AUTOMATION",
            Self::Collusion => "COLLUSION",
            Self::IntentionalStalling => "INTENTIONAL_STALLING",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "IMPOSSIBLE_ORDER" => Some(Self::ImpossibleOrder),
            "AUTOMATION" => Some(Self::Automation),
            "COLLUSION" => Some(Self::Collusion),
            "INTENTIONAL_STALLING" => Some(Self::IntentionalStalling),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegritySignal {
    pub id: Uuid,
    pub subject_identity_id: Uuid,
    pub room_id: Option<Uuid>,
    pub kind: IntegritySignalKind,
    pub severity: u8,
    pub confidence: f64,
    pub evidence: serde_json::Value,
    pub occurrences: u32,
    pub first_observed_at: DateTime<Utc>,
    pub last_observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewIntegritySignal {
    pub id: Uuid,
    pub subject_identity_id: Uuid,
    pub room_id: Option<Uuid>,
    pub kind: IntegritySignalKind,
    pub severity: u8,
    pub confidence: f64,
    pub evidence: serde_json::Value,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegritySignalPage {
    pub signals: Vec<IntegritySignal>,
    pub next_before: Option<DateTime<Utc>>,
}
