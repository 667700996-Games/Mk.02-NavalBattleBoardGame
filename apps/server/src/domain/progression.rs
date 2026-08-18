use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::domain::{LiveContentView, RankedProfile};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerProgression {
    pub account_id: Option<Uuid>,
    pub handle: String,
    pub level: u32,
    pub rank_title: String,
    pub total_xp: u64,
    pub level_xp: u64,
    pub xp_to_next_level: u64,
    pub games_played: u32,
    pub wins: u32,
    pub losses: u32,
    pub total_shots: u32,
    pub total_hits: u32,
    pub total_ships_sunk: u32,
    pub ranked: Option<RankedProfile>,
    pub achievements: Vec<AchievementProgress>,
    pub missions: Vec<MissionProgress>,
    pub live_content: LiveContentView,
    pub calculated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AchievementProgress {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub progress: u32,
    pub target: u32,
    pub unlocked: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MissionCadence {
    Daily,
    Weekly,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionProgress {
    pub id: &'static str,
    pub cadence: MissionCadence,
    pub title: &'static str,
    pub description: &'static str,
    pub progress: u32,
    pub target: u32,
    pub reward_xp: u32,
    pub completed: bool,
    pub claimed: bool,
    pub claimable: bool,
}
