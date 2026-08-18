use std::collections::HashSet;

use chrono::{DateTime, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};

pub const LIVE_CONTENT_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveSeason {
    pub id: String,
    pub title: String,
    pub description: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveEvent {
    pub id: String,
    pub title: String,
    pub description: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveFeatureFlags {
    pub missions_enabled: bool,
    pub event_banner_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveTuning {
    pub daily_deployment_reward_xp: u32,
    pub daily_accuracy_reward_xp: u32,
    pub weekly_supremacy_reward_xp: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveContentPayload {
    pub activate_at: DateTime<Utc>,
    pub season: LiveSeason,
    pub events: Vec<LiveEvent>,
    pub feature_flags: LiveFeatureFlags,
    pub tuning: LiveTuning,
    pub change_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveContentRevision {
    pub schema_version: u16,
    pub revision: u64,
    pub activate_at: DateTime<Utc>,
    pub season: LiveSeason,
    pub events: Vec<LiveEvent>,
    pub feature_flags: LiveFeatureFlags,
    pub tuning: LiveTuning,
    pub operator_id: String,
    pub change_note: String,
    pub created_at: DateTime<Utc>,
    pub rolled_back_from_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveContentIssue {
    pub code: &'static str,
    pub path: String,
    pub message: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveContentValidation {
    pub valid: bool,
    pub candidate_revision: u64,
    pub issues: Vec<LiveContentIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LiveContentStatus {
    Upcoming,
    Active,
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSeasonView {
    #[serde(flatten)]
    pub season: LiveSeason,
    pub status: LiveContentStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveEventView {
    #[serde(flatten)]
    pub event: LiveEvent,
    pub status: LiveContentStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveContentView {
    pub revision: u64,
    pub season: LiveSeasonView,
    pub events: Vec<LiveEventView>,
    pub feature_flags: LiveFeatureFlags,
    pub server_time: DateTime<Utc>,
}

fn status(
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> LiveContentStatus {
    if now < starts_at {
        LiveContentStatus::Upcoming
    } else if now < ends_at {
        LiveContentStatus::Active
    } else {
        LiveContentStatus::Ended
    }
}

impl LiveContentView {
    pub fn from_revision(revision: &LiveContentRevision, now: DateTime<Utc>) -> Self {
        let events = if revision.feature_flags.event_banner_enabled {
            revision
                .events
                .iter()
                .filter_map(|event| {
                    let status = status(event.starts_at, event.ends_at, now);
                    (status != LiveContentStatus::Ended).then(|| LiveEventView {
                        event: event.clone(),
                        status,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        Self {
            revision: revision.revision,
            season: LiveSeasonView {
                season: revision.season.clone(),
                status: status(revision.season.starts_at, revision.season.ends_at, now),
            },
            events,
            feature_flags: revision.feature_flags,
            server_time: now,
        }
    }
}

fn issue(
    issues: &mut Vec<LiveContentIssue>,
    code: &'static str,
    path: impl Into<String>,
    message: &'static str,
) {
    issues.push(LiveContentIssue {
        code,
        path: path.into(),
        message,
    });
}

fn valid_id(value: &str) -> bool {
    (3..=32).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn valid_copy(value: &str, minimum: usize, maximum: usize) -> bool {
    let length = value.chars().count();
    let trimmed_length = value.trim().chars().count();
    (minimum..=maximum).contains(&length)
        && trimmed_length >= minimum
        && !value.chars().any(char::is_control)
}

fn database_timestamp(value: DateTime<Utc>) -> DateTime<Utc> {
    value
        .with_nanosecond((value.nanosecond() / 1_000) * 1_000)
        .expect("microsecond timestamp precision must be valid")
}

impl LiveContentRevision {
    pub fn from_payload(
        revision: u64,
        payload: LiveContentPayload,
        operator_id: String,
        created_at: DateTime<Utc>,
        rolled_back_from_revision: Option<u64>,
    ) -> Self {
        Self {
            schema_version: LIVE_CONTENT_SCHEMA_VERSION,
            revision,
            activate_at: database_timestamp(payload.activate_at),
            season: payload.season,
            events: payload.events,
            feature_flags: payload.feature_flags,
            tuning: payload.tuning,
            operator_id,
            change_note: payload.change_note,
            created_at: database_timestamp(created_at),
            rolled_back_from_revision,
        }
    }

    pub fn payload_for_rollback(
        &self,
        activate_at: DateTime<Utc>,
        change_note: String,
    ) -> LiveContentPayload {
        LiveContentPayload {
            activate_at,
            season: self.season.clone(),
            events: self.events.clone(),
            feature_flags: self.feature_flags,
            tuning: self.tuning,
            change_note,
        }
    }

    pub fn validate(&self, now: DateTime<Utc>) -> LiveContentValidation {
        let mut issues = Vec::new();
        if self.schema_version != LIVE_CONTENT_SCHEMA_VERSION {
            issue(
                &mut issues,
                "UNSUPPORTED_SCHEMA",
                "schemaVersion",
                "지원되는 라이브 콘텐츠 스키마가 아닙니다.",
            );
        }
        if self.revision == 0 {
            issue(
                &mut issues,
                "INVALID_REVISION",
                "revision",
                "발행 리비전은 1 이상이어야 합니다.",
            );
        }
        if self.activate_at < now - Duration::minutes(5)
            || self.activate_at > now + Duration::days(90)
        {
            issue(
                &mut issues,
                "UNSAFE_ACTIVATION_WINDOW",
                "activateAt",
                "활성화 시각은 현재 5분 전부터 90일 후 사이여야 합니다.",
            );
        }
        if !valid_copy(&self.operator_id, 1, 64) {
            issue(
                &mut issues,
                "INVALID_OPERATOR",
                "operatorId",
                "운영자 식별자는 1~64자의 일반 텍스트여야 합니다.",
            );
        }
        if !valid_copy(&self.change_note, 8, 256) {
            issue(
                &mut issues,
                "INVALID_CHANGE_NOTE",
                "changeNote",
                "변경 사유는 8~256자의 일반 텍스트여야 합니다.",
            );
        }
        if !valid_id(&self.season.id) {
            issue(
                &mut issues,
                "INVALID_ID",
                "season.id",
                "식별자는 3~32자의 영문 대문자, 숫자, 밑줄만 사용할 수 있습니다.",
            );
        }
        if !valid_copy(&self.season.title, 2, 64) {
            issue(
                &mut issues,
                "INVALID_COPY",
                "season.title",
                "시즌 제목은 2~64자여야 합니다.",
            );
        }
        if !valid_copy(&self.season.description, 8, 240) {
            issue(
                &mut issues,
                "INVALID_COPY",
                "season.description",
                "시즌 설명은 8~240자여야 합니다.",
            );
        }
        let season_duration = self.season.ends_at - self.season.starts_at;
        if season_duration < Duration::days(7) || season_duration > Duration::days(200) {
            issue(
                &mut issues,
                "INVALID_SEASON_WINDOW",
                "season",
                "시즌은 7~200일 범위여야 합니다.",
            );
        }
        if self.season.ends_at <= self.activate_at {
            issue(
                &mut issues,
                "EXPIRED_SEASON",
                "season.endsAt",
                "활성화 시점에 종료된 시즌은 발행할 수 없습니다.",
            );
        }
        if self.events.len() > 12 {
            issue(
                &mut issues,
                "TOO_MANY_EVENTS",
                "events",
                "한 리비전에는 이벤트를 최대 12개까지 포함할 수 있습니다.",
            );
        }
        let mut event_ids = HashSet::new();
        for (index, event) in self.events.iter().enumerate() {
            let path = |field: &str| format!("events[{index}].{field}");
            if !valid_id(&event.id) {
                issue(
                    &mut issues,
                    "INVALID_ID",
                    path("id"),
                    "식별자는 3~32자의 영문 대문자, 숫자, 밑줄만 사용할 수 있습니다.",
                );
            } else if !event_ids.insert(&event.id) {
                issue(
                    &mut issues,
                    "DUPLICATE_EVENT_ID",
                    path("id"),
                    "이벤트 식별자는 리비전 안에서 고유해야 합니다.",
                );
            }
            if !valid_copy(&event.title, 2, 64) {
                issue(
                    &mut issues,
                    "INVALID_COPY",
                    path("title"),
                    "이벤트 제목은 2~64자여야 합니다.",
                );
            }
            if !valid_copy(&event.description, 8, 240) {
                issue(
                    &mut issues,
                    "INVALID_COPY",
                    path("description"),
                    "이벤트 설명은 8~240자여야 합니다.",
                );
            }
            let event_duration = event.ends_at - event.starts_at;
            if event_duration <= Duration::zero() || event_duration > Duration::days(45) {
                issue(
                    &mut issues,
                    "INVALID_EVENT_WINDOW",
                    path("endsAt"),
                    "이벤트 기간은 0일 초과 45일 이하여야 합니다.",
                );
            }
            if event.starts_at < self.season.starts_at || event.ends_at > self.season.ends_at {
                issue(
                    &mut issues,
                    "EVENT_OUTSIDE_SEASON",
                    format!("events[{index}]"),
                    "이벤트 기간은 시즌 기간 안에 있어야 합니다.",
                );
            }
        }
        for (path, value, minimum, maximum) in [
            (
                "tuning.dailyDeploymentRewardXp",
                self.tuning.daily_deployment_reward_xp,
                25,
                500,
            ),
            (
                "tuning.dailyAccuracyRewardXp",
                self.tuning.daily_accuracy_reward_xp,
                25,
                750,
            ),
            (
                "tuning.weeklySupremacyRewardXp",
                self.tuning.weekly_supremacy_reward_xp,
                100,
                2_500,
            ),
        ] {
            if !(minimum..=maximum).contains(&value) {
                issue(
                    &mut issues,
                    "TUNING_OUT_OF_RANGE",
                    path,
                    "튜닝 값이 허용된 안전 범위를 벗어났습니다.",
                );
            }
        }
        LiveContentValidation {
            valid: issues.is_empty(),
            candidate_revision: self.revision,
            issues,
        }
    }
}

fn utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("baseline live-content timestamp must be valid")
        .with_timezone(&Utc)
}

pub fn baseline_live_content() -> LiveContentRevision {
    LiveContentRevision {
        schema_version: LIVE_CONTENT_SCHEMA_VERSION,
        revision: 0,
        activate_at: utc("2026-08-01T00:00:00Z"),
        season: LiveSeason {
            id: "FOUNDERS_SEASON".to_string(),
            title: "창립 함대 시즌".to_string(),
            description: "정식 함대 지휘 체계를 확립하고 첫 시즌 전공을 기록하십시오.".to_string(),
            starts_at: utc("2026-08-01T00:00:00Z"),
            ends_at: utc("2026-10-31T23:59:59Z"),
        },
        events: vec![LiveEvent {
            id: "COMMANDER_MUSTER".to_string(),
            title: "지휘관 소집령".to_string(),
            description: "일일·주간 임무를 완수해 창립 시즌 함대의 작전 기록을 확장하십시오."
                .to_string(),
            starts_at: utc("2026-08-18T00:00:00Z"),
            ends_at: utc("2026-09-01T00:00:00Z"),
        }],
        feature_flags: LiveFeatureFlags {
            missions_enabled: true,
            event_banner_enabled: true,
        },
        tuning: LiveTuning {
            daily_deployment_reward_xp: 100,
            daily_accuracy_reward_xp: 150,
            weekly_supremacy_reward_xp: 400,
        },
        operator_id: "SYSTEM_BASELINE".to_string(),
        change_note: "Built-in safe baseline content".to_string(),
        created_at: utc("2026-08-01T00:00:00Z"),
        rolled_back_from_revision: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_unsafe_tuning_duplicate_events_and_backdated_activation() {
        let now = utc("2026-08-18T12:00:00Z");
        let mut revision = baseline_live_content();
        revision.revision = 1;
        revision.activate_at = now - Duration::minutes(6);
        revision.tuning.weekly_supremacy_reward_xp = 10_000;
        revision.events.push(revision.events[0].clone());

        let validation = revision.validate(now);
        assert!(!validation.valid);
        let codes: Vec<_> = validation.issues.iter().map(|issue| issue.code).collect();
        assert!(codes.contains(&"UNSAFE_ACTIVATION_WINDOW"));
        assert!(codes.contains(&"DUPLICATE_EVENT_ID"));
        assert!(codes.contains(&"TUNING_OUT_OF_RANGE"));
    }

    #[test]
    fn public_view_hides_ended_events_and_honors_the_banner_kill_switch() {
        let now = utc("2026-08-20T00:00:00Z");
        let mut revision = baseline_live_content();
        revision.events.push(LiveEvent {
            id: "ENDED_EVENT".to_string(),
            title: "종료 이벤트".to_string(),
            description: "이미 종료되어 플레이어에게 노출되지 않는 이벤트입니다.".to_string(),
            starts_at: utc("2026-08-02T00:00:00Z"),
            ends_at: utc("2026-08-03T00:00:00Z"),
        });
        let view = LiveContentView::from_revision(&revision, now);
        assert_eq!(view.events.len(), 1);
        assert_eq!(view.events[0].event.id, "COMMANDER_MUSTER");

        revision.feature_flags.event_banner_enabled = false;
        assert!(
            LiveContentView::from_revision(&revision, now)
                .events
                .is_empty()
        );
    }
}
