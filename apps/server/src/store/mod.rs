mod memory;
mod postgres;

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    domain::{
        AccountSession, ActivePenalty, BalancePin, GameResult, GameRoom, IntegritySignal,
        IntegritySignalKind, IntegritySignalPage, LiveContentRevision, MatchmakingCriteria,
        MatchmakingQuality, ModerationAction, ModerationCasePage, NewIntegritySignal,
        NewModerationAction, NewPlayerReport, NewSupportAction, PlayerAccount,
        RankedLeaderboardPage, RankedProfile, ReportStatus, RoomSummary, SafetyRelationship,
        SupportAccountSnapshot, SupportAction, UserSession,
    },
    error::GameError,
};

pub use memory::MemoryStore;
pub use postgres::{
    DatabaseVerification, DeletionLedgerApplyReport, PostgresRedisStore, PrivacyDeletionLedger,
    PrivacyDeletionTombstone,
};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameHistoryItem {
    pub room_id: Uuid,
    pub room_name: String,
    pub self_player_id: Uuid,
    pub balance: BalancePin,
    pub result: GameResult,
}

#[derive(Debug, Clone)]
pub struct MatchmakingClaim {
    pub id: Uuid,
    pub opponent: UserSession,
    pub opponent_queued_at: DateTime<Utc>,
    pub opponent_criteria: MatchmakingCriteria,
    pub quality: MatchmakingQuality,
}

#[derive(Debug, Clone)]
pub struct MatchmakingEnqueueResult {
    pub queued_at: DateTime<Utc>,
    pub criteria: MatchmakingCriteria,
    pub claim: Option<MatchmakingClaim>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchmakingQueueEntry {
    pub queued_at: DateTime<Utc>,
    pub criteria: MatchmakingCriteria,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedRating {
    pub rating: i32,
    pub matches_played: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MatchmakingQueueStats {
    pub queued: u64,
    pub ranked_queued: u64,
    pub oldest_age_seconds: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetentionStats {
    pub sessions_deleted: u64,
    pub rooms_deleted: u64,
    pub matchmaking_entries_deleted: u64,
    pub moderation_cases_deleted: u64,
    pub integrity_signals_deleted: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDeletionStats {
    pub sessions_deleted: u64,
    pub rewards_deleted: u64,
    pub relationships_deleted: u64,
    pub reports_deleted: u64,
    pub integrity_signals_deleted: u64,
    pub rooms_anonymized: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountDeletionScope {
    LiveRequest,
    RestoredBackup,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionReward {
    pub mission_id: String,
    pub period_key: String,
    pub xp: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomAuthorityLease {
    pub room_id: Uuid,
    pub owner_instance_id: Uuid,
    pub fencing_token: u64,
}

#[async_trait]
pub trait GameStore: Send + Sync {
    async fn health_check(&self) -> Result<(), GameError>;
    async fn save_session(&self, session: &UserSession) -> Result<(), GameError>;
    async fn session_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<UserSession>, GameError>;
    async fn update_session_room(
        &self,
        session_id: Uuid,
        room_id: Option<Uuid>,
    ) -> Result<(), GameError>;
    async fn delete_session(&self, session_id: Uuid) -> Result<(), GameError>;
    async fn create_account(
        &self,
        session_id: Uuid,
        account: &PlayerAccount,
        recovery_key_hash: &str,
        next_token_hash: &str,
    ) -> Result<(), GameError>;
    async fn account_by_credentials(
        &self,
        account_id: Uuid,
        recovery_key_hash: &str,
    ) -> Result<Option<PlayerAccount>, GameError>;
    async fn sessions_for_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<AccountSession>, GameError>;
    async fn delete_account_session(
        &self,
        account_id: Uuid,
        session_id: Uuid,
    ) -> Result<bool, GameError>;
    async fn support_account(
        &self,
        query: &str,
    ) -> Result<Option<SupportAccountSnapshot>, GameError>;
    async fn revoke_account_sessions_for_support(
        &self,
        request: &NewSupportAction,
    ) -> Result<SupportAction, GameError>;
    async fn export_account_data(
        &self,
        account_id: Uuid,
        request_id: Uuid,
        subject_fingerprint: &str,
        generated_at: DateTime<Utc>,
    ) -> Result<serde_json::Value, GameError>;
    async fn delete_account_data(
        &self,
        account_id: Uuid,
        request_id: Uuid,
        subject_fingerprint: &str,
        known_room_ids: &[Uuid],
        deleted_at: DateTime<Utc>,
        scope: AccountDeletionScope,
    ) -> Result<AccountDeletionStats, GameError>;
    async fn mission_rewards(&self, account_id: Uuid) -> Result<Vec<MissionReward>, GameError>;
    async fn claim_mission_reward(
        &self,
        account_id: Uuid,
        mission_id: &str,
        period_key: &str,
        xp: u32,
    ) -> Result<bool, GameError>;
    async fn latest_live_content(&self) -> Result<Option<LiveContentRevision>, GameError>;
    async fn active_live_content(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Option<LiveContentRevision>, GameError>;
    async fn live_content_revision(
        &self,
        revision: u64,
    ) -> Result<Option<LiveContentRevision>, GameError>;
    async fn live_content_history(
        &self,
        limit: usize,
    ) -> Result<Vec<LiveContentRevision>, GameError>;
    async fn commit_live_content(
        &self,
        expected_revision: u64,
        candidate: &LiveContentRevision,
    ) -> Result<bool, GameError>;
    async fn identity_for_session(&self, session_id: Uuid) -> Result<Option<Uuid>, GameError>;
    async fn set_safety_relationship(
        &self,
        actor_identity_id: Uuid,
        relationship: SafetyRelationship,
    ) -> Result<(), GameError>;
    async fn safety_relationships(
        &self,
        actor_identity_id: Uuid,
    ) -> Result<Vec<SafetyRelationship>, GameError>;
    async fn safety_relationship_between(
        &self,
        actor_identity_id: Uuid,
        target_identity_id: Uuid,
    ) -> Result<Option<SafetyRelationship>, GameError>;
    async fn create_player_report(&self, report: &NewPlayerReport) -> Result<(), GameError>;
    async fn moderation_cases(
        &self,
        search: Option<&str>,
        status: Option<ReportStatus>,
        before: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<ModerationCasePage, GameError>;
    async fn apply_moderation_action(
        &self,
        action: &NewModerationAction,
    ) -> Result<ModerationAction, GameError>;
    async fn active_penalty(
        &self,
        identity_id: Uuid,
        session_id: Uuid,
    ) -> Result<Option<ActivePenalty>, GameError>;
    async fn session_ids_for_identity(&self, identity_id: Uuid) -> Result<Vec<Uuid>, GameError>;
    async fn record_integrity_signal(
        &self,
        signal: &NewIntegritySignal,
    ) -> Result<IntegritySignal, GameError>;
    async fn integrity_signals(
        &self,
        search: Option<&str>,
        kind: Option<IntegritySignalKind>,
        before: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<IntegritySignalPage, GameError>;
    async fn suspicious_short_match_count(
        &self,
        first_identity_id: Uuid,
        second_identity_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<u64, GameError>;
    async fn save_room(&self, room: &mut GameRoom) -> Result<(), GameError>;
    async fn acquire_room_authority(
        &self,
        _room_id: Uuid,
        _owner_instance_id: Uuid,
        _lease_duration: Duration,
    ) -> Result<Option<RoomAuthorityLease>, GameError> {
        Ok(None)
    }
    async fn save_room_fenced(
        &self,
        room: &mut GameRoom,
        _lease: RoomAuthorityLease,
    ) -> Result<(), GameError> {
        self.save_room(room).await
    }
    async fn release_room_authority(&self, _lease: RoomAuthorityLease) -> Result<(), GameError> {
        Ok(())
    }
    async fn room_by_id(&self, id: Uuid) -> Result<Option<GameRoom>, GameError>;
    async fn room_by_id_authoritative(&self, id: Uuid) -> Result<Option<GameRoom>, GameError> {
        self.room_by_id(id).await
    }
    async fn room_by_code(&self, code: &str) -> Result<Option<GameRoom>, GameError>;
    async fn active_rooms(&self) -> Result<Vec<GameRoom>, GameError>;
    async fn list_public_rooms(&self) -> Result<Vec<RoomSummary>, GameError>;
    async fn list_spectatable_rooms(&self) -> Result<Vec<RoomSummary>, GameError>;
    async fn history_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Vec<GameHistoryItem>, GameError>;
    async fn enqueue_matchmaking(
        &self,
        session: &UserSession,
        criteria: MatchmakingCriteria,
    ) -> Result<MatchmakingEnqueueResult, GameError>;
    async fn complete_matchmaking(
        &self,
        claim_id: Uuid,
        room: &mut GameRoom,
    ) -> Result<(), GameError>;
    async fn release_matchmaking_claim(&self, claim_id: Uuid) -> Result<(), GameError>;
    async fn cancel_matchmaking(&self, session_id: Uuid) -> Result<bool, GameError>;
    async fn matchmaking_entry(
        &self,
        session_id: Uuid,
    ) -> Result<Option<MatchmakingQueueEntry>, GameError>;
    async fn ranked_rating(&self, account_id: Uuid) -> Result<RankedRating, GameError>;
    async fn ranked_profile(
        &self,
        account_id: Uuid,
        season_id: &str,
        season_starts_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<RankedProfile, GameError>;
    async fn ranked_leaderboard_visibility(&self, account_id: Uuid) -> Result<bool, GameError>;
    async fn set_ranked_leaderboard_visibility(
        &self,
        account_id: Uuid,
        visible: bool,
    ) -> Result<(), GameError>;
    async fn ranked_leaderboard(
        &self,
        season_id: &str,
        active_season_id: &str,
        archived: bool,
        cursor: Option<Uuid>,
        limit: usize,
        now: DateTime<Utc>,
    ) -> Result<RankedLeaderboardPage, GameError>;
    async fn matchmaking_queue_stats(&self) -> Result<MatchmakingQueueStats, GameError>;
    async fn prune_expired_data(
        &self,
        inactive_session_before: DateTime<Utc>,
        completed_room_before: DateTime<Utc>,
        abandoned_matchmaking_before: DateTime<Utc>,
        closed_moderation_before: DateTime<Utc>,
        integrity_signal_before: DateTime<Utc>,
    ) -> Result<RetentionStats, GameError>;
    fn kind(&self) -> &'static str;
}
