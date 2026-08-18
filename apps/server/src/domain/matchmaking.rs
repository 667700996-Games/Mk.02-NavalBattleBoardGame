use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::GameError;

pub const DEFAULT_RANKED_RATING: i32 = 1_500;
pub const MIN_RANKED_RATING: i32 = 0;
pub const MAX_RANKED_RATING: i32 = 4_000;
pub const MAX_RANKED_LATENCY_MS: u16 = 300;
pub const RECENT_OPPONENT_LOOKBACK_MINUTES: i64 = 30;
pub const REMATCH_RELAX_SECONDS: u64 = 90;
pub const REMATCH_STARVATION_SECONDS: u64 = 180;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MatchmakingPool {
    #[default]
    Casual,
    Ranked,
}

impl MatchmakingPool {
    pub const fn as_db_str(self) -> &'static str {
        match self {
            Self::Casual => "CASUAL",
            Self::Ranked => "RANKED",
        }
    }

    pub fn from_db_str(value: &str) -> Result<Self, GameError> {
        match value {
            "CASUAL" => Ok(Self::Casual),
            "RANKED" => Ok(Self::Ranked),
            _ => Err(GameError::Internal),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MatchmakingRegion {
    #[default]
    Auto,
    Korea,
    Japan,
    SoutheastAsia,
    NorthAmericaWest,
    NorthAmericaEast,
    Europe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegionGroup {
    AsiaPacific,
    NorthAmerica,
    Europe,
}

impl MatchmakingRegion {
    pub const fn as_db_str(self) -> &'static str {
        match self {
            Self::Auto => "AUTO",
            Self::Korea => "KOREA",
            Self::Japan => "JAPAN",
            Self::SoutheastAsia => "SOUTHEAST_ASIA",
            Self::NorthAmericaWest => "NORTH_AMERICA_WEST",
            Self::NorthAmericaEast => "NORTH_AMERICA_EAST",
            Self::Europe => "EUROPE",
        }
    }

    pub fn from_db_str(value: &str) -> Result<Self, GameError> {
        match value {
            "AUTO" => Ok(Self::Auto),
            "KOREA" => Ok(Self::Korea),
            "JAPAN" => Ok(Self::Japan),
            "SOUTHEAST_ASIA" => Ok(Self::SoutheastAsia),
            "NORTH_AMERICA_WEST" => Ok(Self::NorthAmericaWest),
            "NORTH_AMERICA_EAST" => Ok(Self::NorthAmericaEast),
            "EUROPE" => Ok(Self::Europe),
            _ => Err(GameError::Internal),
        }
    }

    fn group(self) -> Option<RegionGroup> {
        match self {
            Self::Auto => None,
            Self::Korea | Self::Japan | Self::SoutheastAsia => Some(RegionGroup::AsiaPacific),
            Self::NorthAmericaWest | Self::NorthAmericaEast => Some(RegionGroup::NorthAmerica),
            Self::Europe => Some(RegionGroup::Europe),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MatchmakingPreferences {
    #[serde(default)]
    pub pool: MatchmakingPool,
    #[serde(default)]
    pub region: MatchmakingRegion,
    #[serde(default)]
    pub latency_ms: Option<u16>,
}

impl MatchmakingPreferences {
    pub fn validate(self) -> Result<Self, GameError> {
        match self.pool {
            MatchmakingPool::Casual => {
                if self.region != MatchmakingRegion::Auto || self.latency_ms.is_some() {
                    return Err(GameError::InvalidRequest);
                }
            }
            MatchmakingPool::Ranked => {
                if self.region == MatchmakingRegion::Auto
                    || !self
                        .latency_ms
                        .is_some_and(|latency| (1..=MAX_RANKED_LATENCY_MS).contains(&latency))
                {
                    return Err(GameError::InvalidRequest);
                }
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MatchmakingCriteria {
    pub pool: MatchmakingPool,
    pub region: MatchmakingRegion,
    pub latency_ms: u16,
    pub rating: Option<i32>,
    pub season_key: Option<Uuid>,
    pub party_id: Uuid,
    pub party_size: u8,
}

impl MatchmakingCriteria {
    pub fn casual(session_id: Uuid) -> Self {
        Self {
            pool: MatchmakingPool::Casual,
            region: MatchmakingRegion::Auto,
            latency_ms: 0,
            rating: None,
            season_key: None,
            party_id: session_id,
            party_size: 1,
        }
    }

    pub fn ranked(
        account_id: Uuid,
        region: MatchmakingRegion,
        latency_ms: u16,
        rating: i32,
        season_key: Uuid,
    ) -> Result<Self, GameError> {
        let criteria = Self {
            pool: MatchmakingPool::Ranked,
            region,
            latency_ms,
            rating: Some(rating),
            season_key: Some(season_key),
            party_id: account_id,
            party_size: 1,
        };
        criteria.validate()?;
        Ok(criteria)
    }

    pub fn validate(self) -> Result<Self, GameError> {
        match self.pool {
            MatchmakingPool::Casual => {
                if self.region != MatchmakingRegion::Auto
                    || self.latency_ms != 0
                    || self.rating.is_some()
                    || self.season_key.is_some()
                    || self.party_size != 1
                {
                    return Err(GameError::InvalidRequest);
                }
            }
            MatchmakingPool::Ranked => {
                if self.region == MatchmakingRegion::Auto
                    || !(1..=MAX_RANKED_LATENCY_MS).contains(&self.latency_ms)
                    || !self.rating.is_some_and(|rating| {
                        (MIN_RANKED_RATING..=MAX_RANKED_RATING).contains(&rating)
                    })
                    || self.season_key.is_none()
                    || self.party_size != 1
                {
                    return Err(GameError::InvalidRequest);
                }
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MatchmakingSearchPhase {
    Exact,
    Regional,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakingSearchWindow {
    pub phase: MatchmakingSearchPhase,
    pub rating_delta: u16,
    pub max_latency_ms: u16,
    pub elapsed_seconds: u64,
}

impl MatchmakingSearchWindow {
    pub fn at(queued_at: DateTime<Utc>, now: DateTime<Utc>) -> Self {
        let elapsed_seconds = now.signed_duration_since(queued_at).num_seconds().max(0) as u64;
        let (phase, rating_delta, max_latency_ms) = match elapsed_seconds {
            0..=29 => (MatchmakingSearchPhase::Exact, 100, 120),
            30..=89 => (MatchmakingSearchPhase::Regional, 250, 200),
            _ => (MatchmakingSearchPhase::Global, 500, MAX_RANKED_LATENCY_MS),
        };
        Self {
            phase,
            rating_delta,
            max_latency_ms,
            elapsed_seconds,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakingQuality {
    pub pool: MatchmakingPool,
    pub phase: MatchmakingSearchPhase,
    pub rating_delta: u16,
    pub max_reported_latency_ms: u16,
    pub party_size: u8,
    #[serde(default)]
    pub recent_pairings: u16,
    #[serde(default)]
    pub rematch_relaxed: bool,
    #[serde(default)]
    pub shared_wait_seconds: u64,
    #[serde(default)]
    pub wait_skew_seconds: u64,
}

impl MatchmakingQuality {
    pub const fn rematch_priority(self) -> u16 {
        if self.shared_wait_seconds >= REMATCH_STARVATION_SECONDS {
            0
        } else {
            self.recent_pairings
        }
    }
}

pub fn matchmaking_quality(
    first: MatchmakingCriteria,
    first_queued_at: DateTime<Utc>,
    second: MatchmakingCriteria,
    second_queued_at: DateTime<Utc>,
    now: DateTime<Utc>,
    recent_pairings: u16,
) -> Option<MatchmakingQuality> {
    if first.pool != second.pool
        || first.party_id == second.party_id
        || first.party_size != second.party_size
    {
        return None;
    }
    if first.pool == MatchmakingPool::Casual {
        return Some(MatchmakingQuality {
            pool: MatchmakingPool::Casual,
            phase: MatchmakingSearchPhase::Exact,
            rating_delta: 0,
            max_reported_latency_ms: 0,
            party_size: 1,
            recent_pairings: 0,
            rematch_relaxed: false,
            shared_wait_seconds: 0,
            wait_skew_seconds: 0,
        });
    }

    if first.season_key != second.season_key {
        return None;
    }

    let first_window = MatchmakingSearchWindow::at(first_queued_at, now);
    let second_window = MatchmakingSearchWindow::at(second_queued_at, now);
    let shared_wait_seconds = first_window
        .elapsed_seconds
        .min(second_window.elapsed_seconds);
    if recent_pairings > 0 && shared_wait_seconds < REMATCH_RELAX_SECONDS {
        return None;
    }
    if first.latency_ms > first_window.max_latency_ms
        || second.latency_ms > second_window.max_latency_ms
    {
        return None;
    }
    let first_rating = first.rating?;
    let second_rating = second.rating?;
    let rating_delta = first_rating.abs_diff(second_rating);
    if rating_delta > u32::from(first_window.rating_delta.min(second_window.rating_delta)) {
        return None;
    }

    let same_region = first.region == second.region;
    let same_group =
        first.region.group().is_some() && first.region.group() == second.region.group();
    let regions_allowed = same_region
        || (same_group
            && first_window.phase >= MatchmakingSearchPhase::Regional
            && second_window.phase >= MatchmakingSearchPhase::Regional)
        || (first_window.phase == MatchmakingSearchPhase::Global
            && second_window.phase == MatchmakingSearchPhase::Global);
    if !regions_allowed {
        return None;
    }

    Some(MatchmakingQuality {
        pool: MatchmakingPool::Ranked,
        phase: first_window.phase.min(second_window.phase),
        rating_delta: u16::try_from(rating_delta).unwrap_or(u16::MAX),
        max_reported_latency_ms: first.latency_ms.max(second.latency_ms),
        party_size: first.party_size,
        recent_pairings,
        rematch_relaxed: recent_pairings > 0,
        shared_wait_seconds,
        wait_skew_seconds: first_window
            .elapsed_seconds
            .abs_diff(second_window.elapsed_seconds),
    })
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn ranked(
        party_id: Uuid,
        region: MatchmakingRegion,
        latency_ms: u16,
        rating: i32,
    ) -> MatchmakingCriteria {
        MatchmakingCriteria::ranked(party_id, region, latency_ms, rating, Uuid::nil()).unwrap()
    }

    #[test]
    fn ranked_preferences_reject_auto_region_unbounded_latency_and_client_party_data() {
        assert!(
            MatchmakingPreferences {
                pool: MatchmakingPool::Ranked,
                region: MatchmakingRegion::Auto,
                latency_ms: Some(50),
            }
            .validate()
            .is_err()
        );
        assert!(
            MatchmakingPreferences {
                pool: MatchmakingPool::Ranked,
                region: MatchmakingRegion::Korea,
                latency_ms: Some(301),
            }
            .validate()
            .is_err()
        );
        assert!(
            serde_json::from_value::<MatchmakingPreferences>(serde_json::json!({
                "pool": "RANKED",
                "region": "KOREA",
                "latencyMs": 40,
                "partyId": Uuid::new_v4()
            }))
            .is_err()
        );
    }

    #[test]
    fn ranked_search_widens_only_when_both_players_accept_the_scope() {
        let now = Utc::now();
        let korea = ranked(Uuid::new_v4(), MatchmakingRegion::Korea, 80, 1_500);
        let japan = ranked(Uuid::new_v4(), MatchmakingRegion::Japan, 90, 1_680);

        assert!(matchmaking_quality(korea, now, japan, now, now, 0).is_none());
        assert!(
            matchmaking_quality(korea, now - Duration::seconds(31), japan, now, now, 0).is_none(),
            "one player's wider window must not override the other player's exact search"
        );
        let regional = matchmaking_quality(
            korea,
            now - Duration::seconds(31),
            japan,
            now - Duration::seconds(31),
            now,
            0,
        )
        .unwrap();
        assert_eq!(regional.phase, MatchmakingSearchPhase::Regional);
        assert_eq!(regional.rating_delta, 180);

        let europe = ranked(Uuid::new_v4(), MatchmakingRegion::Europe, 100, 1_820);
        assert!(
            matchmaking_quality(
                korea,
                now - Duration::seconds(91),
                europe,
                now - Duration::seconds(91),
                now,
                0,
            )
            .is_some(),
            "both global windows accept a cross-region 320-point match"
        );
    }

    #[test]
    fn ranked_search_rejects_same_party_rating_and_latency_outside_mutual_bounds() {
        let now = Utc::now();
        let party_id = Uuid::new_v4();
        let first = ranked(party_id, MatchmakingRegion::Korea, 80, 1_500);
        let same_party = ranked(party_id, MatchmakingRegion::Korea, 80, 1_510);
        assert!(matchmaking_quality(first, now, same_party, now, now, 0).is_none());

        let far_rating = ranked(Uuid::new_v4(), MatchmakingRegion::Korea, 80, 1_700);
        assert!(matchmaking_quality(first, now, far_rating, now, now, 0).is_none());

        let high_latency = ranked(Uuid::new_v4(), MatchmakingRegion::Korea, 180, 1_510);
        assert!(matchmaking_quality(first, now, high_latency, now, now, 0).is_none());
        assert!(
            matchmaking_quality(
                first,
                now - Duration::seconds(31),
                high_latency,
                now - Duration::seconds(31),
                now,
                0,
            )
            .is_some()
        );
    }

    #[test]
    fn ranked_rematches_require_mutual_global_wait_and_eventually_restore_fifo_priority() {
        let now = Utc::now();
        let first = ranked(Uuid::new_v4(), MatchmakingRegion::Korea, 60, 1_500);
        let second = ranked(Uuid::new_v4(), MatchmakingRegion::Korea, 65, 1_510);

        assert!(matchmaking_quality(first, now, second, now, now, 1).is_none());
        assert!(
            matchmaking_quality(
                first,
                now - Duration::seconds(91),
                second,
                now - Duration::seconds(89),
                now,
                1,
            )
            .is_none(),
            "one player's global wait must not force the other into a rematch"
        );

        let relaxed = matchmaking_quality(
            first,
            now - Duration::seconds(95),
            second,
            now - Duration::seconds(91),
            now,
            2,
        )
        .unwrap();
        assert_eq!(relaxed.phase, MatchmakingSearchPhase::Global);
        assert_eq!(relaxed.recent_pairings, 2);
        assert!(relaxed.rematch_relaxed);
        assert_eq!(relaxed.shared_wait_seconds, 91);
        assert_eq!(relaxed.wait_skew_seconds, 4);
        assert_eq!(relaxed.rematch_priority(), 2);

        let starved = matchmaking_quality(
            first,
            now - Duration::seconds(185),
            second,
            now - Duration::seconds(181),
            now,
            2,
        )
        .unwrap();
        assert_eq!(starved.rematch_priority(), 0);
    }
}
