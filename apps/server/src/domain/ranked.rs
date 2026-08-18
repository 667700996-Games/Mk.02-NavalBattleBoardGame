use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DEFAULT_RANKED_RATING, MAX_RANKED_RATING, MIN_RANKED_RATING};

pub const RANKED_PLACEMENT_MATCHES: u32 = 5;
pub const RANKED_DECAY_GRACE_DAYS: i64 = 14;
pub const RANKED_DECAY_INTERVAL_DAYS: i64 = 7;
pub const RANKED_DECAY_POINTS: i32 = 25;
pub const RANKED_DECAY_THRESHOLD: i32 = 2_100;
pub const RANKED_DECAY_FLOOR: i32 = 1_800;
pub const RANKED_LEADERBOARD_DEFAULT_LIMIT: usize = 20;
pub const RANKED_LEADERBOARD_MAX_LIMIT: usize = 50;
pub const RANKED_LEADERBOARD_FINALIZATION_HOURS: i64 = 24;

const RANKED_SEASON_NAMESPACE: Uuid = Uuid::from_u128(0x58b9_d700_9c42_4bf4_a95d_16aa_e04e_7249);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RankedTier {
    Provisional,
    Bronze,
    Silver,
    Gold,
    Platinum,
    Diamond,
    Admiral,
}

impl RankedTier {
    pub const fn for_standing(rating: i32, matches_played: u32) -> Self {
        if matches_played < RANKED_PLACEMENT_MATCHES {
            return Self::Provisional;
        }
        match rating {
            ..=1_199 => Self::Bronze,
            1_200..=1_499 => Self::Silver,
            1_500..=1_799 => Self::Gold,
            1_800..=2_099 => Self::Platinum,
            2_100..=2_399 => Self::Diamond,
            _ => Self::Admiral,
        }
    }

    pub const fn season_reward_xp(self) -> u32 {
        match self {
            Self::Provisional => 0,
            Self::Bronze => 500,
            Self::Silver => 750,
            Self::Gold => 1_000,
            Self::Platinum => 1_500,
            Self::Diamond => 2_000,
            Self::Admiral => 3_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankedMatchContext {
    pub season_id: String,
    pub content_revision: u64,
}

impl RankedMatchContext {
    pub fn season_key(&self) -> Uuid {
        ranked_season_key(&self.season_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedStandingRecord {
    pub season_id: String,
    pub rating: i32,
    pub matches_played: u32,
    pub wins: u32,
    pub losses: u32,
    pub peak_rating: i32,
    pub last_match_at: Option<DateTime<Utc>>,
    pub decay_steps_applied: u32,
    pub season_reward_issued_at: Option<DateTime<Utc>>,
}

impl RankedStandingRecord {
    pub fn new(season_id: String, seed_rating: i32) -> Self {
        let rating = seed_rating.clamp(MIN_RANKED_RATING, MAX_RANKED_RATING);
        Self {
            season_id,
            rating,
            matches_played: 0,
            wins: 0,
            losses: 0,
            peak_rating: rating,
            last_match_at: None,
            decay_steps_applied: 0,
            season_reward_issued_at: None,
        }
    }

    pub fn tier(&self) -> RankedTier {
        RankedTier::for_standing(self.rating, self.matches_played)
    }

    pub fn placement_matches_remaining(&self) -> u32 {
        RANKED_PLACEMENT_MATCHES.saturating_sub(self.matches_played)
    }

    pub fn next_decay_at(&self) -> Option<DateTime<Utc>> {
        if self.matches_played < RANKED_PLACEMENT_MATCHES || self.rating < RANKED_DECAY_THRESHOLD {
            return None;
        }
        self.last_match_at.map(|last_match| {
            last_match
                + Duration::days(RANKED_DECAY_GRACE_DAYS)
                + Duration::days(i64::from(self.decay_steps_applied) * RANKED_DECAY_INTERVAL_DAYS)
        })
    }

    pub fn apply_inactivity_decay(&mut self, now: DateTime<Utc>) -> i32 {
        if self.matches_played < RANKED_PLACEMENT_MATCHES || self.rating < RANKED_DECAY_THRESHOLD {
            return 0;
        }
        let Some(last_match_at) = self.last_match_at else {
            return 0;
        };
        let inactive_days = now.signed_duration_since(last_match_at).num_days();
        if inactive_days < RANKED_DECAY_GRACE_DAYS {
            return 0;
        }
        let due_steps = 1_u32.saturating_add(
            u32::try_from((inactive_days - RANKED_DECAY_GRACE_DAYS) / RANKED_DECAY_INTERVAL_DAYS)
                .unwrap_or(u32::MAX),
        );
        let new_steps = due_steps.saturating_sub(self.decay_steps_applied);
        if new_steps == 0 {
            return 0;
        }
        let before = self.rating;
        let decay = i32::try_from(new_steps)
            .unwrap_or(i32::MAX)
            .saturating_mul(RANKED_DECAY_POINTS);
        self.rating = self.rating.saturating_sub(decay).max(RANKED_DECAY_FLOOR);
        self.decay_steps_applied = due_steps;
        self.rating - before
    }

    pub fn record_result(
        &mut self,
        opponent_rating: i32,
        won: bool,
        finished_at: DateTime<Utc>,
    ) -> RankedRatingChange {
        let before = self.rating;
        let expected = 1.0 / (1.0 + 10_f64.powf(f64::from(opponent_rating - before) / 400.0));
        let score = if won { 1.0 } else { 0.0 };
        let k_factor = if self.matches_played < RANKED_PLACEMENT_MATCHES {
            64.0
        } else {
            32.0
        };
        let mut delta = (k_factor * (score - expected)).round() as i32;
        if won {
            delta = delta.max(1);
        } else {
            delta = delta.min(-1);
        }
        self.rating = before
            .saturating_add(delta)
            .clamp(MIN_RANKED_RATING, MAX_RANKED_RATING);
        delta = self.rating - before;
        self.matches_played = self.matches_played.saturating_add(1);
        if won {
            self.wins = self.wins.saturating_add(1);
        } else {
            self.losses = self.losses.saturating_add(1);
        }
        self.peak_rating = self.peak_rating.max(self.rating);
        self.last_match_at = Some(finished_at);
        self.decay_steps_applied = 0;
        RankedRatingChange {
            rating_before: before,
            rating_after: self.rating,
            delta,
            placement_completed: self.matches_played == RANKED_PLACEMENT_MATCHES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankedRatingChange {
    pub rating_before: i32,
    pub rating_after: i32,
    pub delta: i32,
    pub placement_completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedProfile {
    pub season_id: String,
    pub rating: i32,
    pub matches_played: u32,
    pub wins: u32,
    pub losses: u32,
    pub peak_rating: i32,
    pub tier: RankedTier,
    pub placement_matches_remaining: u32,
    pub last_match_at: Option<DateTime<Utc>>,
    pub next_decay_at: Option<DateTime<Utc>>,
    pub decay_points_applied: u32,
    pub reward_xp_earned: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedLeaderboardEntry {
    pub rank: u32,
    pub handle: String,
    pub rating: i32,
    pub tier: RankedTier,
    pub matches_played: u32,
    pub wins: u32,
    pub losses: u32,
    pub peak_rating: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedLeaderboardSeason {
    pub season_id: String,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedLeaderboardPage {
    pub season_id: String,
    pub archived: bool,
    pub generated_at: DateTime<Utc>,
    pub entries: Vec<RankedLeaderboardEntry>,
    pub next_cursor: Option<Uuid>,
    pub available_seasons: Vec<RankedLeaderboardSeason>,
}

impl RankedProfile {
    pub fn from_record(record: &RankedStandingRecord, reward_xp_earned: u64) -> Self {
        Self {
            season_id: record.season_id.clone(),
            rating: record.rating,
            matches_played: record.matches_played,
            wins: record.wins,
            losses: record.losses,
            peak_rating: record.peak_rating,
            tier: record.tier(),
            placement_matches_remaining: record.placement_matches_remaining(),
            last_match_at: record.last_match_at,
            next_decay_at: record.next_decay_at(),
            decay_points_applied: record
                .decay_steps_applied
                .saturating_mul(RANKED_DECAY_POINTS as u32),
            reward_xp_earned,
        }
    }
}

pub fn ranked_season_key(season_id: &str) -> Uuid {
    Uuid::new_v5(&RANKED_SEASON_NAMESPACE, season_id.as_bytes())
}

pub fn next_season_seed(previous_rating: Option<i32>) -> i32 {
    previous_rating.map_or(DEFAULT_RANKED_RATING, |rating| {
        (DEFAULT_RANKED_RATING + (rating - DEFAULT_RANKED_RATING) / 2).clamp(1_000, 2_000)
    })
}

pub const fn ranked_match_reward_xp(won: bool) -> u32 {
    if won { 100 } else { 40 }
}

pub const fn ranked_placement_reward_xp() -> u32 {
    500
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_uses_a_larger_k_factor_then_reveals_a_tier() {
        let finished_at = Utc::now();
        let mut standing = RankedStandingRecord::new("S1".to_string(), 1_500);
        for _ in 0..4 {
            let change = standing.record_result(1_500, true, finished_at);
            assert!(!change.placement_completed);
            assert_eq!(standing.tier(), RankedTier::Provisional);
        }
        let change = standing.record_result(1_500, true, finished_at);
        assert!(change.placement_completed);
        assert_ne!(standing.tier(), RankedTier::Provisional);
        let established_delta = standing
            .record_result(standing.rating, true, finished_at)
            .delta;
        assert_eq!(established_delta, 16);
    }

    #[test]
    fn inactivity_decay_is_idempotent_and_stops_at_the_floor() {
        let now = Utc::now();
        let mut standing = RankedStandingRecord {
            season_id: "S1".to_string(),
            rating: 2_150,
            matches_played: 20,
            wins: 10,
            losses: 10,
            peak_rating: 2_300,
            last_match_at: Some(now - Duration::days(28)),
            decay_steps_applied: 0,
            season_reward_issued_at: None,
        };
        assert_eq!(standing.apply_inactivity_decay(now), -75);
        assert_eq!(standing.apply_inactivity_decay(now), 0);
        assert_eq!(standing.rating, 2_075);

        standing.rating = 2_150;
        standing.last_match_at = Some(now - Duration::days(400));
        standing.decay_steps_applied = 0;
        assert_eq!(standing.apply_inactivity_decay(now), -350);
        assert_eq!(standing.rating, RANKED_DECAY_FLOOR);
    }

    #[test]
    fn season_keys_and_soft_resets_are_stable() {
        assert_eq!(ranked_season_key("S1"), ranked_season_key("S1"));
        assert_ne!(ranked_season_key("S1"), ranked_season_key("S2"));
        assert_eq!(next_season_seed(None), 1_500);
        assert_eq!(next_season_seed(Some(2_500)), 2_000);
        assert_eq!(next_season_seed(Some(900)), 1_200);
    }
}
