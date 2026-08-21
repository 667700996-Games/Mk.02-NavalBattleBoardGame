use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use std::collections::{HashMap, HashSet};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    domain::{
        AccountSession, ActivePenalty, ChatMessageType, DEFAULT_RANKED_RATING, FinishReason,
        GameRoom, IntegritySignal, IntegritySignalKind, IntegritySignalPage, LiveContentRevision,
        MatchmakingCriteria, MatchmakingPool, ModerationAction, ModerationActionKind,
        ModerationCase, ModerationCasePage, NewIntegritySignal, NewModerationAction,
        NewPlayerReport, NewSupportAction, PlayerAccount, PlayerReport,
        RANKED_LEADERBOARD_MAX_LIMIT, RECENT_OPPONENT_LOOKBACK_MINUTES, RankedLeaderboardEntry,
        RankedLeaderboardPage, RankedLeaderboardSeason, RankedProfile, RankedStandingRecord,
        RankedTier, ReportStatus, RoomStatus, RoomSummary, RoomVisibility, SafetyRelationship,
        SupportAccountSnapshot, SupportAction, UserSession, matchmaking_quality, next_season_seed,
        ranked_match_reward_xp, ranked_placement_reward_xp, ranked_season_key,
    },
    error::GameError,
};

use super::{
    AccountDeletionScope, AccountDeletionStats, GameHistoryItem, GameStore, MatchmakingClaim,
    MatchmakingEnqueueResult, MatchmakingQueueEntry, MatchmakingQueueStats, MissionReward,
    RankedRating, RetentionStats,
};

#[derive(Debug, Clone)]
struct MatchmakingEntry {
    session: UserSession,
    queued_at: DateTime<Utc>,
    criteria: MatchmakingCriteria,
    claim_id: Option<Uuid>,
    claimed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct LeaderboardSnapshotEntry {
    rank: u32,
    account_id: Uuid,
    rating: i32,
    matches_played: u32,
    wins: u32,
    losses: u32,
    peak_rating: i32,
}

#[derive(Debug, Clone)]
struct LeaderboardSnapshot {
    id: Uuid,
    season_id: String,
    generated_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    archived: bool,
    entries: Vec<LeaderboardSnapshotEntry>,
}

#[derive(Debug, Clone, Copy)]
struct LeaderboardCursor {
    snapshot_id: Uuid,
    after_rank: u32,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct MemoryStore {
    sessions_by_hash: DashMap<String, UserSession>,
    session_hash_by_id: DashMap<Uuid, String>,
    accounts: DashMap<Uuid, (PlayerAccount, String)>,
    account_id_by_handle: DashMap<String, Uuid>,
    account_mutations: Mutex<()>,
    support_actions: DashMap<Uuid, SupportAction>,
    mission_rewards: DashMap<(Uuid, String, String), u32>,
    live_content_revisions: Mutex<Vec<LiveContentRevision>>,
    safety_relationships: DashMap<(Uuid, Uuid), SafetyRelationship>,
    safety_mutations: Mutex<()>,
    player_reports: DashMap<Uuid, PlayerReport>,
    moderation_actions: DashMap<Uuid, ModerationAction>,
    moderation_mutations: Mutex<()>,
    integrity_signals: DashMap<Uuid, IntegritySignal>,
    integrity_mutations: Mutex<()>,
    privacy_requests: DashMap<Uuid, serde_json::Value>,
    rooms: DashMap<Uuid, GameRoom>,
    matchmaking: Mutex<HashMap<Uuid, MatchmakingEntry>>,
    ranked_ratings: DashMap<Uuid, RankedRating>,
    ranked_rating_seasons: DashMap<Uuid, String>,
    ranked_standings: DashMap<(Uuid, String), RankedStandingRecord>,
    ranked_rewards: DashMap<(Uuid, String, String, String), u32>,
    ranked_settlements: DashMap<Uuid, ()>,
    ranked_mutations: Mutex<()>,
    leaderboard_visibility: DashMap<Uuid, bool>,
    leaderboard_snapshots: DashMap<Uuid, LeaderboardSnapshot>,
    leaderboard_archives: DashMap<String, Uuid>,
    leaderboard_cursors: DashMap<Uuid, LeaderboardCursor>,
    leaderboard_mutations: Mutex<()>,
}

impl MemoryStore {
    fn stored_identity_for_session(&self, session_id: Uuid) -> Option<Uuid> {
        self.session_hash_by_id
            .get(&session_id)
            .and_then(|hash| self.sessions_by_hash.get(hash.value()))
            .map(|session| session.account_id.unwrap_or(session.id))
    }

    fn recent_pairing_count(
        &self,
        first_identity_id: Uuid,
        second_identity_id: Uuid,
        since: DateTime<Utc>,
    ) -> u16 {
        let count = self
            .rooms
            .iter()
            .filter(|room| {
                let Some(result) = room.game.as_ref().and_then(|game| game.result.as_ref()) else {
                    return false;
                };
                if result.finished_at < since {
                    return false;
                }
                let identities: Vec<_> = room
                    .players
                    .iter()
                    .filter_map(|player| self.stored_identity_for_session(player.session_id))
                    .collect();
                identities.contains(&first_identity_id) && identities.contains(&second_identity_id)
            })
            .count();
        u16::try_from(count).unwrap_or(u16::MAX)
    }

    fn has_leaderboard_penalty(&self, account_id: Uuid, now: DateTime<Utc>) -> bool {
        let mut identities = vec![account_id];
        identities.extend(
            self.sessions_by_hash
                .iter()
                .filter(|session| session.account_id == Some(account_id))
                .map(|session| session.id),
        );
        let actions: Vec<_> = self
            .moderation_actions
            .iter()
            .map(|action| action.value().clone())
            .collect();
        let reversed: HashSet<_> = actions
            .iter()
            .filter_map(|action| action.reverses_action_id)
            .collect();
        actions.iter().any(|action| {
            identities.contains(&action.target_identity_id)
                && !reversed.contains(&action.id)
                && (action.action == ModerationActionKind::Ban
                    || (action.action == ModerationActionKind::Suspend
                        && action.expires_at.is_some_and(|expires_at| expires_at > now)))
        })
    }

    fn build_leaderboard_snapshot(
        &self,
        season_id: &str,
        archived: bool,
        now: DateTime<Utc>,
    ) -> LeaderboardSnapshot {
        let mut standings: Vec<_> = self
            .ranked_standings
            .iter()
            .filter(|standing| {
                standing.key().1 == season_id
                    && standing.matches_played >= crate::domain::RANKED_PLACEMENT_MATCHES
                    && standing.wins.saturating_add(standing.losses) == standing.matches_played
                    && self.accounts.contains_key(&standing.key().0)
            })
            .map(|standing| (standing.key().0, standing.value().clone()))
            .collect();
        standings.sort_by(|(left_id, left), (right_id, right)| {
            right
                .rating
                .cmp(&left.rating)
                .then_with(|| right.wins.cmp(&left.wins))
                .then_with(|| right.peak_rating.cmp(&left.peak_rating))
                .then_with(|| left.matches_played.cmp(&right.matches_played))
                .then_with(|| left_id.cmp(right_id))
        });
        let entries = standings
            .into_iter()
            .enumerate()
            .filter_map(|(index, (account_id, standing))| {
                Some(LeaderboardSnapshotEntry {
                    rank: u32::try_from(index).ok()?.checked_add(1)?,
                    account_id,
                    rating: standing.rating,
                    matches_played: standing.matches_played,
                    wins: standing.wins,
                    losses: standing.losses,
                    peak_rating: standing.peak_rating,
                })
            })
            .collect();
        LeaderboardSnapshot {
            id: Uuid::new_v4(),
            season_id: season_id.to_string(),
            generated_at: now,
            expires_at: (!archived).then_some(now + Duration::minutes(5)),
            archived,
            entries,
        }
    }

    fn standing_or_seed(&self, account_id: Uuid, season_id: &str) -> RankedStandingRecord {
        if let Some(standing) = self
            .ranked_standings
            .get(&(account_id, season_id.to_string()))
        {
            return standing.clone();
        }
        let previous_rating = self
            .ranked_standings
            .iter()
            .filter(|standing| standing.key().0 == account_id)
            .max_by_key(|standing| standing.last_match_at)
            .map(|standing| standing.rating)
            .or_else(|| {
                self.ranked_ratings
                    .get(&account_id)
                    .map(|rating| rating.rating)
            });
        RankedStandingRecord::new(season_id.to_string(), next_season_seed(previous_rating))
    }

    fn issue_prior_season_rewards(
        &self,
        account_id: Uuid,
        current_season_id: &str,
        current_season_starts_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) {
        let prior_keys: Vec<_> = self
            .ranked_standings
            .iter()
            .filter(|standing| {
                standing.key().0 == account_id
                    && standing.key().1 != current_season_id
                    && standing.matches_played >= crate::domain::RANKED_PLACEMENT_MATCHES
                    && standing.season_reward_issued_at.is_none()
                    && standing
                        .last_match_at
                        .is_some_and(|last_match| last_match < current_season_starts_at)
            })
            .map(|standing| standing.key().clone())
            .collect();
        for key in prior_keys {
            if let Some(mut standing) = self.ranked_standings.get_mut(&key) {
                let xp = standing.tier().season_reward_xp();
                if xp > 0 {
                    self.ranked_rewards.insert(
                        (
                            account_id,
                            "RANKED_SEASON".to_string(),
                            standing.season_id.clone(),
                            standing.season_id.clone(),
                        ),
                        xp,
                    );
                }
                standing.season_reward_issued_at = Some(now);
            }
        }
    }

    async fn settle_ranked_room(&self, room: &GameRoom) -> Result<(), GameError> {
        let Some(context) = room.ranked_match.as_ref() else {
            return Ok(());
        };
        let Some(result) = room.game.as_ref().and_then(|game| game.result.as_ref()) else {
            return Ok(());
        };
        let _guard = self.ranked_mutations.lock().await;
        if self.ranked_settlements.contains_key(&room.id) {
            return Ok(());
        }
        let mut participants = Vec::with_capacity(2);
        for player in &room.players {
            let account_id = self
                .session_hash_by_id
                .get(&player.session_id)
                .and_then(|hash| {
                    self.sessions_by_hash
                        .get(hash.value())
                        .and_then(|session| session.account_id)
                })
                .ok_or(GameError::Internal)?;
            participants.push((account_id, player.id));
        }
        if participants.len() != 2 || participants[0].0 == participants[1].0 {
            return Err(GameError::Internal);
        }
        let mut first = self.standing_or_seed(participants[0].0, &context.season_id);
        let mut second = self.standing_or_seed(participants[1].0, &context.season_id);
        let first_won = result.winner_id == participants[0].1;
        let second_won = result.winner_id == participants[1].1;
        if first_won == second_won {
            return Err(GameError::Internal);
        }
        let first_change = first.record_result(second.rating, first_won, result.finished_at);
        let second_change =
            second.record_result(first_change.rating_before, second_won, result.finished_at);
        for (account_id, standing, change, won) in [
            (participants[0].0, first, first_change, first_won),
            (participants[1].0, second, second_change, second_won),
        ] {
            self.ranked_ratings.insert(
                account_id,
                RankedRating {
                    rating: standing.rating,
                    matches_played: standing.matches_played,
                },
            );
            self.ranked_rating_seasons
                .insert(account_id, context.season_id.clone());
            self.ranked_standings
                .insert((account_id, context.season_id.clone()), standing);
            self.ranked_rewards.insert(
                (
                    account_id,
                    "RANKED_MATCH".to_string(),
                    room.id.to_string(),
                    context.season_id.clone(),
                ),
                ranked_match_reward_xp(won),
            );
            if change.placement_completed {
                self.ranked_rewards.insert(
                    (
                        account_id,
                        "RANKED_PLACEMENT".to_string(),
                        context.season_id.clone(),
                        context.season_id.clone(),
                    ),
                    ranked_placement_reward_xp(),
                );
            }
        }
        self.ranked_settlements.insert(room.id, ());
        Ok(())
    }
}

#[async_trait]
impl GameStore for MemoryStore {
    async fn health_check(&self) -> Result<(), GameError> {
        Ok(())
    }

    async fn save_session(&self, session: &UserSession) -> Result<(), GameError> {
        self.session_hash_by_id
            .insert(session.id, session.token_hash.clone());
        self.sessions_by_hash
            .insert(session.token_hash.clone(), session.clone());
        Ok(())
    }

    async fn session_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<UserSession>, GameError> {
        Ok(self
            .sessions_by_hash
            .get(token_hash)
            .map(|entry| entry.clone()))
    }

    async fn update_session_room(
        &self,
        session_id: Uuid,
        room_id: Option<Uuid>,
    ) -> Result<(), GameError> {
        let hash = self
            .session_hash_by_id
            .get(&session_id)
            .ok_or(GameError::Unauthorized)?
            .clone();
        let mut session = self
            .sessions_by_hash
            .get_mut(&hash)
            .ok_or(GameError::Unauthorized)?;
        session.current_room_id = room_id;
        session.last_seen_at = chrono::Utc::now();
        Ok(())
    }

    async fn delete_session(&self, session_id: Uuid) -> Result<(), GameError> {
        if let Some((_, hash)) = self.session_hash_by_id.remove(&session_id) {
            self.sessions_by_hash.remove(&hash);
        }
        Ok(())
    }

    async fn create_account(
        &self,
        session_id: Uuid,
        account: &PlayerAccount,
        recovery_key_hash: &str,
        next_token_hash: &str,
    ) -> Result<(), GameError> {
        let _guard = self.account_mutations.lock().await;
        let session_hash = self
            .session_hash_by_id
            .get(&session_id)
            .ok_or(GameError::Unauthorized)?
            .clone();
        let normalized_handle = account.handle.to_lowercase();
        if self.account_id_by_handle.contains_key(&normalized_handle) {
            return Err(GameError::AccountHandleTaken);
        }
        let mut session = {
            let session = self
                .sessions_by_hash
                .get(&session_hash)
                .ok_or(GameError::Unauthorized)?;
            if session.account_id.is_some() {
                return Err(GameError::InvalidState);
            }
            session.value().clone()
        };
        session.account_id = Some(account.id);
        session.nickname = account.handle.clone();
        session.token_hash = next_token_hash.to_string();
        session.last_seen_at = Utc::now();
        self.account_id_by_handle
            .insert(normalized_handle, account.id);
        self.accounts
            .insert(account.id, (account.clone(), recovery_key_hash.to_string()));
        let previous_token_hash = self
            .session_hash_by_id
            .insert(session_id, next_token_hash.to_string())
            .ok_or(GameError::Unauthorized)?;
        self.sessions_by_hash.remove(&previous_token_hash);
        self.sessions_by_hash
            .insert(next_token_hash.to_string(), session);
        Ok(())
    }

    async fn account_by_credentials(
        &self,
        account_id: Uuid,
        recovery_key_hash: &str,
    ) -> Result<Option<PlayerAccount>, GameError> {
        Ok(self.accounts.get(&account_id).and_then(|entry| {
            (entry.value().1 == recovery_key_hash).then(|| entry.value().0.clone())
        }))
    }

    async fn sessions_for_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<AccountSession>, GameError> {
        let mut sessions: Vec<_> = self
            .sessions_by_hash
            .iter()
            .filter(|session| session.account_id == Some(account_id))
            .map(|session| AccountSession {
                id: session.id,
                nickname: session.nickname.clone(),
                created_at: session.created_at,
                last_seen_at: session.last_seen_at,
                current_room_id: session.current_room_id,
            })
            .collect();
        sessions.sort_by_key(|session| std::cmp::Reverse(session.last_seen_at));
        Ok(sessions)
    }

    async fn delete_account_session(
        &self,
        account_id: Uuid,
        session_id: Uuid,
    ) -> Result<bool, GameError> {
        let Some(hash) = self
            .session_hash_by_id
            .get(&session_id)
            .map(|entry| entry.clone())
        else {
            return Ok(false);
        };
        if self
            .sessions_by_hash
            .get(&hash)
            .is_none_or(|session| session.account_id != Some(account_id))
        {
            return Ok(false);
        }
        self.session_hash_by_id.remove(&session_id);
        self.sessions_by_hash.remove(&hash);
        Ok(true)
    }

    async fn support_account(
        &self,
        query: &str,
    ) -> Result<Option<SupportAccountSnapshot>, GameError> {
        let account_id = Uuid::parse_str(query).ok().or_else(|| {
            self.account_id_by_handle
                .get(&query.to_lowercase())
                .map(|entry| *entry.value())
        });
        let Some(account_id) = account_id else {
            return Ok(None);
        };
        let Some(account) = self
            .accounts
            .get(&account_id)
            .map(|entry| entry.value().0.clone())
        else {
            return Ok(None);
        };
        let sessions = self.sessions_for_account(account_id).await?;
        let mut actions: Vec<_> = self
            .support_actions
            .iter()
            .filter(|action| action.account_id == account_id)
            .map(|action| action.value().clone())
            .collect();
        actions.sort_by_key(|action| std::cmp::Reverse(action.created_at));
        Ok(Some(SupportAccountSnapshot {
            account,
            sessions,
            actions,
        }))
    }

    async fn revoke_account_sessions_for_support(
        &self,
        request: &NewSupportAction,
    ) -> Result<SupportAction, GameError> {
        let _guard = self.account_mutations.lock().await;
        if !self.accounts.contains_key(&request.account_id) {
            return Err(GameError::SupportAccountNotFound);
        }
        let affected_session_ids: Vec<_> = self
            .session_hash_by_id
            .iter()
            .filter_map(|entry| {
                let id = *entry.key();
                if request.target_session_id.is_some_and(|target| target != id) {
                    return None;
                }
                self.sessions_by_hash
                    .get(entry.value())
                    .is_some_and(|session| session.account_id == Some(request.account_id))
                    .then_some(id)
            })
            .collect();
        if affected_session_ids.is_empty() {
            return Err(GameError::SupportSessionNotFound);
        }
        for id in &affected_session_ids {
            if let Some((_, hash)) = self.session_hash_by_id.remove(id) {
                self.sessions_by_hash.remove(&hash);
            }
        }
        let action = SupportAction {
            id: request.id,
            account_id: request.account_id,
            operator_id: request.operator_id.clone(),
            action: request.action,
            reason: request.reason.clone(),
            target_session_id: request.target_session_id,
            affected_session_ids,
            created_at: request.created_at,
        };
        self.support_actions.insert(action.id, action.clone());
        Ok(action)
    }

    async fn export_account_data(
        &self,
        account_id: Uuid,
        request_id: Uuid,
        subject_fingerprint: &str,
        generated_at: DateTime<Utc>,
    ) -> Result<serde_json::Value, GameError> {
        let account = self
            .accounts
            .get(&account_id)
            .map(|entry| entry.value().0.clone())
            .ok_or(GameError::Unauthorized)?;
        let sessions = self.sessions_for_account(account_id).await?;
        let session_ids: Vec<_> = sessions.iter().map(|session| session.id).collect();
        let mut identities = session_ids.clone();
        identities.push(account_id);
        let history = match session_ids.first() {
            Some(session_id) => self.history_for_session(*session_id).await?,
            None => Vec::new(),
        };
        let rewards = self.mission_rewards(account_id).await?;
        let relationships: Vec<_> = self
            .safety_relationships
            .iter()
            .filter(|relationship| identities.contains(&relationship.key().0))
            .map(|relationship| relationship.value().clone())
            .collect();
        let reports: Vec<_> = self
            .player_reports
            .iter()
            .filter(|report| {
                identities.contains(&report.reporter_identity_id)
                    || identities.contains(&report.target_identity_id)
            })
            .map(|report| {
                serde_json::json!({
                    "id": report.id,
                    "direction": if identities.contains(&report.reporter_identity_id) { "SUBMITTED" } else { "RECEIVED" },
                    "targetNickname": report.target_nickname,
                    "category": report.category,
                    "details": report.details,
                    "evidence": report.evidence,
                    "status": report.status,
                    "createdAt": report.created_at,
                    "updatedAt": report.updated_at,
                })
            })
            .collect();
        let report_ids: Vec<Uuid> = reports
            .iter()
            .filter_map(|report| {
                report["id"]
                    .as_str()
                    .and_then(|id| Uuid::parse_str(id).ok())
            })
            .collect();
        let moderation_actions: Vec<_> = self
            .moderation_actions
            .iter()
            .filter(|action| {
                report_ids.contains(&action.report_id)
                    || identities.contains(&action.target_identity_id)
            })
            .map(|action| action.value().clone())
            .collect();
        let integrity_signals: Vec<_> = self
            .integrity_signals
            .iter()
            .filter(|signal| identities.contains(&signal.subject_identity_id))
            .map(|signal| signal.value().clone())
            .collect();
        let support_actions: Vec<_> = self
            .support_actions
            .iter()
            .filter(|action| action.account_id == account_id)
            .map(|action| action.value().clone())
            .collect();
        let ranked_rating = self
            .ranked_ratings
            .get(&account_id)
            .map(|rating| *rating.value());
        let ranked_standings: Vec<_> = self
            .ranked_standings
            .iter()
            .filter(|standing| standing.key().0 == account_id)
            .map(|standing| standing.value().clone())
            .collect();
        let ranked_rewards: Vec<_> = self
            .ranked_rewards
            .iter()
            .filter(|reward| reward.key().0 == account_id)
            .map(|reward| {
                serde_json::json!({
                    "sourceKind": reward.key().1,
                    "sourceId": reward.key().2,
                    "seasonId": reward.key().3,
                    "xp": reward.value(),
                })
            })
            .collect();
        let ranked_leaderboard_entries: Vec<_> = self
            .leaderboard_snapshots
            .iter()
            .flat_map(|snapshot| {
                snapshot
                    .entries
                    .iter()
                    .filter(|entry| entry.account_id == account_id)
                    .map(|entry| {
                        serde_json::json!({
                            "snapshotId": snapshot.id,
                            "seasonId": snapshot.season_id,
                            "rank": entry.rank,
                            "rating": entry.rating,
                            "matchesPlayed": entry.matches_played,
                            "wins": entry.wins,
                            "losses": entry.losses,
                            "peakRating": entry.peak_rating,
                            "generatedAt": snapshot.generated_at,
                            "archived": snapshot.archived,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let archive = serde_json::json!({
            "formatVersion": 1,
            "requestId": request_id,
            "generatedAt": generated_at,
            "account": account,
            "sessions": sessions,
            "gameHistory": history,
            "progressionRewards": rewards,
            "rankedRating": ranked_rating,
            "rankedStandings": ranked_standings,
            "rankedRewards": ranked_rewards,
            "rankedLeaderboardEntries": ranked_leaderboard_entries,
            "leaderboardVisible": self.leaderboard_visibility.get(&account_id).is_none_or(|visible| *visible),
            "safetyRelationships": relationships,
            "moderationReports": reports,
            "moderationActions": moderation_actions,
            "integritySignals": integrity_signals,
            "supportActions": support_actions,
            "cacheCopies": "No independent data; Redis room cache follows the authoritative room lifecycle.",
            "credentialsExcluded": true,
        });
        self.privacy_requests.insert(
            request_id,
            serde_json::json!({
                "subjectFingerprint": subject_fingerprint,
                "requestType": "EXPORT",
                "status": "COMPLETED",
                "createdAt": generated_at,
                "completedAt": generated_at,
            }),
        );
        Ok(archive)
    }

    async fn delete_account_data(
        &self,
        account_id: Uuid,
        request_id: Uuid,
        subject_fingerprint: &str,
        known_room_ids: &[Uuid],
        deleted_at: DateTime<Utc>,
        _scope: AccountDeletionScope,
    ) -> Result<AccountDeletionStats, GameError> {
        let _account_guard = self.account_mutations.lock().await;
        let account = self
            .accounts
            .get(&account_id)
            .map(|entry| entry.value().0.clone())
            .ok_or(GameError::Unauthorized)?;
        let sessions: Vec<_> = self
            .sessions_by_hash
            .iter()
            .filter(|session| session.account_id == Some(account_id))
            .map(|session| (session.id, session.token_hash.clone()))
            .collect();
        let session_ids: Vec<_> = sessions.iter().map(|(id, _)| *id).collect();
        let deleted_names: Vec<_> = std::iter::once(account.handle.clone())
            .chain(sessions.iter().filter_map(|(_, token_hash)| {
                self.sessions_by_hash
                    .get(token_hash)
                    .map(|session| session.nickname.clone())
            }))
            .collect();
        let mut identities = session_ids.clone();
        identities.push(account_id);
        let affected_room_ids: Vec<_> = self
            .rooms
            .iter()
            .filter(|room| {
                known_room_ids.contains(&room.id)
                    || room
                        .players
                        .iter()
                        .any(|player| session_ids.contains(&player.session_id))
            })
            .map(|room| room.id)
            .collect();
        for room_id in &affected_room_ids {
            let mut room = self.rooms.get_mut(room_id).ok_or(GameError::Internal)?;
            if !matches!(room.status, RoomStatus::Finished | RoomStatus::Cancelled) {
                return Err(GameError::InvalidState);
            }
            let mut deleted_player_ids = Vec::new();
            for player in &mut room.players {
                if session_ids.contains(&player.session_id) {
                    deleted_player_ids.push(player.id);
                    player.session_id = Uuid::new_v4();
                    player.nickname = "Deleted Commander".to_string();
                }
            }
            for message in &mut room.chat_messages {
                let belongs_to_deleted_player = message
                    .player_id
                    .is_some_and(|player_id| deleted_player_ids.contains(&player_id))
                    || deleted_names.contains(&message.nickname);
                for deleted_name in &deleted_names {
                    message.content = message.content.replace(deleted_name, "Deleted Commander");
                }
                if belongs_to_deleted_player {
                    message.nickname = "Deleted Commander".to_string();
                    if message.message_type == ChatMessageType::Text {
                        message.content = "[deleted]".to_string();
                    }
                }
            }
            room.name = "Archived Operation".to_string();
            room.updated_at = deleted_at;
            room.version = room.version.saturating_add(1);
            room.persistence_revision = room.persistence_revision.saturating_add(1);
        }

        let reward_keys: Vec<_> = self
            .mission_rewards
            .iter()
            .filter(|reward| reward.key().0 == account_id)
            .map(|reward| reward.key().clone())
            .collect();
        for key in &reward_keys {
            self.mission_rewards.remove(key);
        }
        let ranked_reward_keys: Vec<_> = self
            .ranked_rewards
            .iter()
            .filter(|reward| reward.key().0 == account_id)
            .map(|reward| reward.key().clone())
            .collect();
        for key in &ranked_reward_keys {
            self.ranked_rewards.remove(key);
        }
        let relationship_keys: Vec<_> = self
            .safety_relationships
            .iter()
            .filter(|relationship| {
                identities.contains(&relationship.key().0)
                    || identities.contains(&relationship.key().1)
            })
            .map(|relationship| *relationship.key())
            .collect();
        for key in &relationship_keys {
            self.safety_relationships.remove(key);
        }
        let _moderation_guard = self.moderation_mutations.lock().await;
        let report_ids: Vec<_> = self
            .player_reports
            .iter()
            .filter(|report| {
                identities.contains(&report.reporter_identity_id)
                    || identities.contains(&report.target_identity_id)
            })
            .map(|report| report.id)
            .collect();
        let action_ids: Vec<_> = self
            .moderation_actions
            .iter()
            .filter(|action| {
                report_ids.contains(&action.report_id)
                    || identities.contains(&action.target_identity_id)
            })
            .map(|action| action.id)
            .collect();
        for action_id in action_ids {
            self.moderation_actions.remove(&action_id);
        }
        for report_id in &report_ids {
            self.player_reports.remove(report_id);
        }
        let _integrity_guard = self.integrity_mutations.lock().await;
        let signal_ids: Vec<_> = self
            .integrity_signals
            .iter()
            .filter(|signal| identities.contains(&signal.subject_identity_id))
            .map(|signal| signal.id)
            .collect();
        for signal_id in &signal_ids {
            self.integrity_signals.remove(signal_id);
        }
        let support_action_ids: Vec<_> = self
            .support_actions
            .iter()
            .filter(|action| action.account_id == account_id)
            .map(|action| action.id)
            .collect();
        for action_id in support_action_ids {
            self.support_actions.remove(&action_id);
        }
        self.matchmaking
            .lock()
            .await
            .retain(|session_id, _| !session_ids.contains(session_id));
        for (session_id, token_hash) in &sessions {
            self.session_hash_by_id.remove(session_id);
            self.sessions_by_hash.remove(token_hash);
        }
        self.account_id_by_handle
            .remove(&account.handle.to_lowercase());
        self.accounts.remove(&account_id);
        self.leaderboard_visibility.remove(&account_id);
        for mut snapshot in self.leaderboard_snapshots.iter_mut() {
            snapshot
                .entries
                .retain(|entry| entry.account_id != account_id);
        }
        self.ranked_ratings.remove(&account_id);
        self.ranked_rating_seasons.remove(&account_id);
        let standing_keys: Vec<_> = self
            .ranked_standings
            .iter()
            .filter(|standing| standing.key().0 == account_id)
            .map(|standing| standing.key().clone())
            .collect();
        for key in standing_keys {
            self.ranked_standings.remove(&key);
        }
        self.privacy_requests.insert(
            request_id,
            serde_json::json!({
                "subjectFingerprint": subject_fingerprint,
                "requestType": "DELETE",
                "status": "COMPLETED",
                "createdAt": deleted_at,
                "completedAt": deleted_at,
            }),
        );
        Ok(AccountDeletionStats {
            sessions_deleted: sessions.len() as u64,
            rewards_deleted: reward_keys.len().saturating_add(ranked_reward_keys.len()) as u64,
            relationships_deleted: relationship_keys.len() as u64,
            reports_deleted: report_ids.len() as u64,
            integrity_signals_deleted: signal_ids.len() as u64,
            rooms_anonymized: affected_room_ids.len() as u64,
        })
    }

    async fn mission_rewards(&self, account_id: Uuid) -> Result<Vec<MissionReward>, GameError> {
        Ok(self
            .mission_rewards
            .iter()
            .filter(|entry| entry.key().0 == account_id)
            .map(|entry| MissionReward {
                mission_id: entry.key().1.clone(),
                period_key: entry.key().2.clone(),
                xp: *entry.value(),
            })
            .collect())
    }

    async fn claim_mission_reward(
        &self,
        account_id: Uuid,
        mission_id: &str,
        period_key: &str,
        xp: u32,
    ) -> Result<bool, GameError> {
        use dashmap::mapref::entry::Entry;

        let key = (account_id, mission_id.to_string(), period_key.to_string());
        match self.mission_rewards.entry(key) {
            Entry::Occupied(_) => Ok(false),
            Entry::Vacant(entry) => {
                entry.insert(xp);
                Ok(true)
            }
        }
    }

    async fn latest_live_content(&self) -> Result<Option<LiveContentRevision>, GameError> {
        Ok(self.live_content_revisions.lock().await.last().cloned())
    }

    async fn active_live_content(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Option<LiveContentRevision>, GameError> {
        Ok(self
            .live_content_revisions
            .lock()
            .await
            .iter()
            .rev()
            .find(|revision| revision.activate_at <= now)
            .cloned())
    }

    async fn live_content_revision(
        &self,
        revision: u64,
    ) -> Result<Option<LiveContentRevision>, GameError> {
        Ok(self
            .live_content_revisions
            .lock()
            .await
            .iter()
            .find(|candidate| candidate.revision == revision)
            .cloned())
    }

    async fn live_content_history(
        &self,
        limit: usize,
    ) -> Result<Vec<LiveContentRevision>, GameError> {
        Ok(self
            .live_content_revisions
            .lock()
            .await
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn commit_live_content(
        &self,
        expected_revision: u64,
        candidate: &LiveContentRevision,
    ) -> Result<bool, GameError> {
        let mut revisions = self.live_content_revisions.lock().await;
        let current = revisions.last().map_or(0, |revision| revision.revision);
        if current != expected_revision
            || expected_revision
                .checked_add(1)
                .is_none_or(|revision| candidate.revision != revision)
        {
            return Ok(false);
        }
        revisions.push(candidate.clone());
        Ok(true)
    }

    async fn identity_for_session(&self, session_id: Uuid) -> Result<Option<Uuid>, GameError> {
        Ok(self.session_hash_by_id.get(&session_id).and_then(|hash| {
            self.sessions_by_hash
                .get(hash.value())
                .map(|session| session.account_id.unwrap_or(session.id))
        }))
    }

    async fn set_safety_relationship(
        &self,
        actor_identity_id: Uuid,
        relationship: SafetyRelationship,
    ) -> Result<(), GameError> {
        let _guard = self.safety_mutations.lock().await;
        let key = (actor_identity_id, relationship.target_identity_id);
        if relationship.has_effect() {
            self.safety_relationships.insert(key, relationship);
        } else {
            self.safety_relationships.remove(&key);
        }
        Ok(())
    }

    async fn safety_relationships(
        &self,
        actor_identity_id: Uuid,
    ) -> Result<Vec<SafetyRelationship>, GameError> {
        let mut relationships: Vec<_> = self
            .safety_relationships
            .iter()
            .filter(|relationship| relationship.key().0 == actor_identity_id)
            .map(|relationship| relationship.value().clone())
            .collect();
        relationships.sort_by_key(|relationship| std::cmp::Reverse(relationship.updated_at));
        Ok(relationships)
    }

    async fn safety_relationship_between(
        &self,
        actor_identity_id: Uuid,
        target_identity_id: Uuid,
    ) -> Result<Option<SafetyRelationship>, GameError> {
        Ok(self
            .safety_relationships
            .get(&(actor_identity_id, target_identity_id))
            .map(|relationship| relationship.value().clone()))
    }

    async fn create_player_report(&self, report: &NewPlayerReport) -> Result<(), GameError> {
        self.player_reports.insert(report.id, report.into());
        Ok(())
    }

    async fn moderation_cases(
        &self,
        search: Option<&str>,
        status: Option<ReportStatus>,
        before: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<ModerationCasePage, GameError> {
        let search = search.map(str::to_lowercase);
        let mut reports: Vec<_> = self
            .player_reports
            .iter()
            .map(|entry| entry.value().clone())
            .filter(|report| status.is_none_or(|status| report.status == status))
            .filter(|report| before.is_none_or(|before| report.created_at < before))
            .filter(|report| {
                search.as_ref().is_none_or(|search| {
                    report.target_nickname.to_lowercase().contains(search)
                        || report.details.to_lowercase().contains(search)
                        || report.evidence.to_string().to_lowercase().contains(search)
                })
            })
            .collect();
        reports.sort_by_key(|report| std::cmp::Reverse(report.created_at));
        let has_more = reports.len() > limit;
        reports.truncate(limit);
        let mut cases = Vec::with_capacity(reports.len());
        for report in reports {
            let mut actions: Vec<_> = self
                .moderation_actions
                .iter()
                .filter(|entry| entry.report_id == report.id)
                .map(|entry| entry.value().clone())
                .collect();
            actions.sort_by_key(|action| action.created_at);
            cases.push(ModerationCase { report, actions });
        }
        let next_before = has_more
            .then(|| cases.last().map(|case| case.report.created_at))
            .flatten();
        Ok(ModerationCasePage { cases, next_before })
    }

    async fn apply_moderation_action(
        &self,
        action: &NewModerationAction,
    ) -> Result<ModerationAction, GameError> {
        let _guard = self.moderation_mutations.lock().await;
        let mut report = self
            .player_reports
            .get_mut(&action.report_id)
            .ok_or(GameError::ReportNotFound)?;
        if action.action == ModerationActionKind::Reverse {
            let reversed_id = action.reverses_action_id.ok_or(GameError::InvalidRequest)?;
            let reversed = self
                .moderation_actions
                .get(&reversed_id)
                .ok_or(GameError::InvalidRequest)?;
            if reversed.report_id != report.id
                || reversed.target_identity_id != report.target_identity_id
                || matches!(
                    reversed.action,
                    ModerationActionKind::Reverse | ModerationActionKind::Dismiss
                )
                || self
                    .moderation_actions
                    .iter()
                    .any(|candidate| candidate.reverses_action_id == Some(reversed_id))
            {
                return Err(GameError::InvalidRequest);
            }
        } else if action.reverses_action_id.is_some() {
            return Err(GameError::InvalidRequest);
        }
        let stored = ModerationAction {
            id: action.id,
            report_id: action.report_id,
            target_identity_id: report.target_identity_id,
            operator_id: action.operator_id.clone(),
            action: action.action,
            reason: action.reason.clone(),
            expires_at: action.expires_at,
            reverses_action_id: action.reverses_action_id,
            created_at: action.created_at,
        };
        report.status = match action.action {
            ModerationActionKind::Dismiss => ReportStatus::Dismissed,
            ModerationActionKind::Reverse => ReportStatus::Reviewing,
            _ => ReportStatus::Actioned,
        };
        report.updated_at = action.created_at;
        self.moderation_actions.insert(stored.id, stored.clone());
        Ok(stored)
    }

    async fn active_penalty(
        &self,
        identity_id: Uuid,
        session_id: Uuid,
    ) -> Result<Option<ActivePenalty>, GameError> {
        let mut identities = vec![identity_id, session_id];
        if identity_id != session_id {
            identities.extend(
                self.sessions_by_hash
                    .iter()
                    .filter(|entry| entry.account_id == Some(identity_id))
                    .map(|entry| entry.id),
            );
        }
        let now = Utc::now();
        let actions: Vec<_> = self
            .moderation_actions
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        let is_reversed = |id| {
            actions
                .iter()
                .any(|candidate| candidate.reverses_action_id == Some(id))
        };
        if actions.iter().any(|action| {
            identities.contains(&action.target_identity_id)
                && action.action == ModerationActionKind::Ban
                && !is_reversed(action.id)
        }) {
            return Ok(Some(ActivePenalty::Banned));
        }
        Ok(actions
            .iter()
            .filter(|action| {
                identities.contains(&action.target_identity_id)
                    && action.action == ModerationActionKind::Suspend
                    && action.expires_at.is_some_and(|expires_at| expires_at > now)
                    && !is_reversed(action.id)
            })
            .filter_map(|action| action.expires_at)
            .max()
            .map(ActivePenalty::Suspended))
    }

    async fn session_ids_for_identity(&self, identity_id: Uuid) -> Result<Vec<Uuid>, GameError> {
        Ok(self
            .sessions_by_hash
            .iter()
            .filter(|entry| entry.id == identity_id || entry.account_id == Some(identity_id))
            .map(|entry| entry.id)
            .collect())
    }

    async fn record_integrity_signal(
        &self,
        signal: &NewIntegritySignal,
    ) -> Result<IntegritySignal, GameError> {
        let _guard = self.integrity_mutations.lock().await;
        if let Some(room_id) = signal.room_id {
            let existing_id = {
                self.integrity_signals.iter().find_map(|entry| {
                    (entry.subject_identity_id == signal.subject_identity_id
                        && entry.room_id == Some(room_id)
                        && entry.kind == signal.kind)
                        .then_some(entry.id)
                })
            };
            if let Some(existing_id) = existing_id {
                let mut existing = self
                    .integrity_signals
                    .get_mut(&existing_id)
                    .ok_or(GameError::Internal)?;
                existing.severity = existing.severity.max(signal.severity);
                existing.confidence = existing.confidence.max(signal.confidence);
                existing.evidence = signal.evidence.clone();
                existing.occurrences = existing.occurrences.saturating_add(1);
                existing.last_observed_at = signal.observed_at;
                return Ok(existing.value().clone());
            }
        }
        let stored = IntegritySignal {
            id: signal.id,
            subject_identity_id: signal.subject_identity_id,
            room_id: signal.room_id,
            kind: signal.kind,
            severity: signal.severity,
            confidence: signal.confidence,
            evidence: signal.evidence.clone(),
            occurrences: 1,
            first_observed_at: signal.observed_at,
            last_observed_at: signal.observed_at,
        };
        self.integrity_signals.insert(stored.id, stored.clone());
        Ok(stored)
    }

    async fn integrity_signals(
        &self,
        search: Option<&str>,
        kind: Option<IntegritySignalKind>,
        before: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<IntegritySignalPage, GameError> {
        let search = search.map(str::to_lowercase);
        let mut signals: Vec<_> = self
            .integrity_signals
            .iter()
            .map(|entry| entry.value().clone())
            .filter(|signal| kind.is_none_or(|kind| signal.kind == kind))
            .filter(|signal| before.is_none_or(|before| signal.last_observed_at < before))
            .filter(|signal| {
                search.as_ref().is_none_or(|search| {
                    signal.subject_identity_id.to_string().contains(search)
                        || signal.evidence.to_string().to_lowercase().contains(search)
                })
            })
            .collect();
        signals.sort_by_key(|signal| {
            (
                std::cmp::Reverse(signal.severity),
                std::cmp::Reverse(signal.last_observed_at),
            )
        });
        let has_more = signals.len() > limit;
        signals.truncate(limit);
        let next_before = has_more
            .then(|| signals.last().map(|signal| signal.last_observed_at))
            .flatten();
        Ok(IntegritySignalPage {
            signals,
            next_before,
        })
    }

    async fn suspicious_short_match_count(
        &self,
        first_identity_id: Uuid,
        second_identity_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<u64, GameError> {
        Ok(self
            .rooms
            .iter()
            .filter(|room| {
                let Some(result) = room.game.as_ref().and_then(|game| game.result.as_ref()) else {
                    return false;
                };
                if result.finished_at < since
                    || result.total_turns > 5
                    || result.finish_reason == FinishReason::FleetDestroyed
                {
                    return false;
                }
                let identities: Vec<_> = room
                    .players
                    .iter()
                    .filter_map(|player| self.stored_identity_for_session(player.session_id))
                    .collect();
                identities.contains(&first_identity_id) && identities.contains(&second_identity_id)
            })
            .count() as u64)
    }

    async fn save_room(&self, room: &mut GameRoom) -> Result<(), GameError> {
        if !room.has_valid_balance_pin() {
            return Err(GameError::InvalidState);
        }
        if self
            .rooms
            .get(&room.id)
            .is_some_and(|stored| stored.persistence_revision != room.persistence_revision)
        {
            return Err(GameError::VersionConflict);
        }
        self.settle_ranked_room(room).await?;
        let next_revision = room.persistence_revision.saturating_add(1);
        let mut persisted = room.clone();
        persisted.persistence_revision = next_revision;
        self.rooms.insert(room.id, persisted);
        room.persistence_revision = next_revision;
        Ok(())
    }

    async fn room_by_id(&self, id: Uuid) -> Result<Option<GameRoom>, GameError> {
        Ok(self.rooms.get(&id).map(|entry| entry.clone()))
    }

    async fn room_by_code(&self, code: &str) -> Result<Option<GameRoom>, GameError> {
        Ok(self
            .rooms
            .iter()
            .find(|entry| entry.code == code)
            .map(|entry| entry.clone()))
    }

    async fn active_rooms(&self) -> Result<Vec<GameRoom>, GameError> {
        Ok(self
            .rooms
            .iter()
            .filter(|entry| !matches!(entry.status, RoomStatus::Finished | RoomStatus::Cancelled))
            .map(|entry| entry.clone())
            .collect())
    }

    async fn list_public_rooms(&self) -> Result<Vec<RoomSummary>, GameError> {
        let mut rooms: Vec<_> = self
            .rooms
            .iter()
            .filter(|entry| {
                entry.visibility == RoomVisibility::Public
                    && entry.status == RoomStatus::WaitingForOpponent
                    && entry.players.len() < 2
            })
            .map(|entry| entry.summary())
            .collect();
        rooms.sort_by_key(|room| std::cmp::Reverse(room.created_at));
        Ok(rooms)
    }

    async fn list_spectatable_rooms(&self) -> Result<Vec<RoomSummary>, GameError> {
        let mut rooms: Vec<_> = self
            .rooms
            .iter()
            .filter(|entry| {
                entry.visibility == RoomVisibility::Public
                    && entry.game.is_some()
                    && entry.status == RoomStatus::Playing
            })
            .map(|entry| entry.summary())
            .collect();
        rooms.sort_by_key(|room| std::cmp::Reverse(room.created_at));
        rooms.truncate(100);
        Ok(rooms)
    }

    async fn history_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Vec<GameHistoryItem>, GameError> {
        let account_id = self
            .session_hash_by_id
            .get(&session_id)
            .and_then(|hash| {
                self.sessions_by_hash
                    .get(hash.value())
                    .map(|session| session.account_id)
            })
            .flatten();
        let identity_session_ids: Vec<_> = self
            .sessions_by_hash
            .iter()
            .filter(|session| {
                session.id == session_id
                    || account_id.is_some_and(|account_id| session.account_id == Some(account_id))
            })
            .map(|session| session.id)
            .collect();
        let mut history = Vec::new();
        for room in self.rooms.iter() {
            if let Some(player) = room
                .players
                .iter()
                .find(|player| identity_session_ids.contains(&player.session_id))
            {
                if let Some(result) = room.game.as_ref().and_then(|game| game.result.clone()) {
                    history.push(GameHistoryItem {
                        room_id: room.id,
                        room_name: room.name.clone(),
                        self_player_id: player.id,
                        balance: room.balance.clone(),
                        result,
                    });
                }
            }
        }
        history.sort_by_key(|item| std::cmp::Reverse(item.result.finished_at));
        Ok(history)
    }

    async fn enqueue_matchmaking(
        &self,
        session: &UserSession,
        criteria: MatchmakingCriteria,
    ) -> Result<MatchmakingEnqueueResult, GameError> {
        let stored_session = self
            .sessions_by_hash
            .get(&session.token_hash)
            .ok_or(GameError::Unauthorized)?;
        if stored_session.current_room_id.is_some() {
            return Err(GameError::AlreadyJoined);
        }
        let stored_session = stored_session.value().clone();
        if stored_session.id != session.id {
            return Err(GameError::Unauthorized);
        }

        let criteria = criteria.validate()?;
        match criteria.pool {
            MatchmakingPool::Casual => {
                if criteria != MatchmakingCriteria::casual(stored_session.id) {
                    return Err(GameError::InvalidRequest);
                }
            }
            MatchmakingPool::Ranked => {
                let account_id = stored_session.account_id.ok_or(GameError::Unauthorized)?;
                if criteria.party_id != account_id
                    || criteria.rating != Some(self.ranked_rating(account_id).await?.rating)
                    || self
                        .ranked_rating_seasons
                        .get(&account_id)
                        .is_none_or(|season| {
                            criteria.season_key != Some(ranked_season_key(&season))
                        })
                {
                    return Err(GameError::InvalidRequest);
                }
            }
        }

        let now = Utc::now();
        let stale_before = now - Duration::seconds(30);
        let abandoned_before = now - Duration::minutes(10);
        let mut queue = self.matchmaking.lock().await;
        queue.retain(|_, entry| entry.claim_id.is_some() || entry.queued_at >= abandoned_before);
        for entry in queue.values_mut() {
            if entry
                .claimed_at
                .is_some_and(|claimed_at| claimed_at < stale_before)
            {
                entry.claim_id = None;
                entry.claimed_at = None;
            }
        }

        let queued_at = if let Some(entry) = queue.get(&session.id) {
            if entry.criteria != criteria {
                return Err(GameError::InvalidState);
            }
            if entry.claim_id.is_some() {
                return Ok(MatchmakingEnqueueResult {
                    queued_at: entry.queued_at,
                    criteria,
                    claim: None,
                });
            }
            entry.queued_at
        } else {
            queue.insert(
                session.id,
                MatchmakingEntry {
                    session: stored_session.clone(),
                    queued_at: now,
                    criteria,
                    claim_id: None,
                    claimed_at: None,
                },
            );
            now
        };

        let own_identity = stored_session.account_id.unwrap_or(stored_session.id);
        let opponent_match = queue
            .iter()
            .filter_map(|(session_id, entry)| {
                if *session_id == session.id || entry.claim_id.is_some() {
                    return None;
                }
                let opponent_identity = entry.session.account_id.unwrap_or(entry.session.id);
                let own_blocks = self
                    .safety_relationships
                    .get(&(own_identity, opponent_identity))
                    .is_some_and(|relationship| relationship.blocked);
                let opponent_blocks = self
                    .safety_relationships
                    .get(&(opponent_identity, own_identity))
                    .is_some_and(|relationship| relationship.blocked);
                if own_blocks || opponent_blocks {
                    return None;
                }
                let recent_pairings = if criteria.pool == MatchmakingPool::Ranked {
                    self.recent_pairing_count(
                        own_identity,
                        opponent_identity,
                        now - Duration::minutes(RECENT_OPPONENT_LOOKBACK_MINUTES),
                    )
                } else {
                    0
                };
                matchmaking_quality(
                    criteria,
                    queued_at,
                    entry.criteria,
                    entry.queued_at,
                    now,
                    recent_pairings,
                )
                .map(|quality| (*session_id, entry.queued_at, quality))
            })
            .min_by_key(|(_, opponent_queued_at, quality)| {
                (
                    quality.rematch_priority(),
                    *opponent_queued_at,
                    quality.rating_delta,
                    quality.max_reported_latency_ms,
                )
            });
        let Some((opponent_id, _, quality)) = opponent_match else {
            return Ok(MatchmakingEnqueueResult {
                queued_at,
                criteria,
                claim: None,
            });
        };

        let claim_id = Uuid::new_v4();
        let opponent = queue
            .get_mut(&opponent_id)
            .expect("selected matchmaking opponent must exist");
        let opponent_queued_at = opponent.queued_at;
        let opponent_criteria = opponent.criteria;
        opponent.claim_id = Some(claim_id);
        opponent.claimed_at = Some(now);
        let opponent = opponent.session.clone();
        let own_entry = queue
            .get_mut(&session.id)
            .expect("queued matchmaking session must exist");
        own_entry.claim_id = Some(claim_id);
        own_entry.claimed_at = Some(now);

        Ok(MatchmakingEnqueueResult {
            queued_at,
            criteria,
            claim: Some(MatchmakingClaim {
                id: claim_id,
                opponent,
                opponent_queued_at,
                opponent_criteria,
                quality,
            }),
        })
    }

    async fn complete_matchmaking(
        &self,
        claim_id: Uuid,
        room: &mut GameRoom,
    ) -> Result<(), GameError> {
        let mut queue = self.matchmaking.lock().await;
        let mut claimed_session_ids: Vec<_> = queue
            .iter()
            .filter(|(_, entry)| entry.claim_id == Some(claim_id))
            .map(|(session_id, _)| *session_id)
            .collect();
        let mut room_session_ids: Vec<_> = room
            .players
            .iter()
            .map(|player| player.session_id)
            .collect();
        claimed_session_ids.sort_unstable();
        room_session_ids.sort_unstable();
        if claimed_session_ids.len() != 2 || claimed_session_ids != room_session_ids {
            return Err(GameError::VersionConflict);
        }

        let session_hashes: Vec<_> = claimed_session_ids
            .iter()
            .map(|session_id| {
                self.session_hash_by_id
                    .get(session_id)
                    .map(|hash| hash.clone())
                    .ok_or(GameError::Unauthorized)
            })
            .collect::<Result<_, _>>()?;
        if session_hashes.iter().any(|hash| {
            self.sessions_by_hash
                .get(hash)
                .is_none_or(|session| session.current_room_id.is_some())
        }) {
            return Err(GameError::AlreadyJoined);
        }
        if self.rooms.contains_key(&room.id) || room.persistence_revision != 0 {
            return Err(GameError::VersionConflict);
        }

        room.persistence_revision = 1;
        self.rooms.insert(room.id, room.clone());
        for hash in session_hashes {
            let mut session = self
                .sessions_by_hash
                .get_mut(&hash)
                .ok_or(GameError::Unauthorized)?;
            session.current_room_id = Some(room.id);
            session.last_seen_at = Utc::now();
        }
        for session_id in claimed_session_ids {
            queue.remove(&session_id);
        }
        Ok(())
    }

    async fn release_matchmaking_claim(&self, claim_id: Uuid) -> Result<(), GameError> {
        let mut queue = self.matchmaking.lock().await;
        for entry in queue.values_mut() {
            if entry.claim_id == Some(claim_id) {
                entry.claim_id = None;
                entry.claimed_at = None;
            }
        }
        Ok(())
    }

    async fn cancel_matchmaking(&self, session_id: Uuid) -> Result<bool, GameError> {
        let mut queue = self.matchmaking.lock().await;
        if queue
            .get(&session_id)
            .is_some_and(|entry| entry.claim_id.is_some())
        {
            return Ok(false);
        }
        Ok(queue.remove(&session_id).is_some())
    }

    async fn matchmaking_entry(
        &self,
        session_id: Uuid,
    ) -> Result<Option<MatchmakingQueueEntry>, GameError> {
        Ok(self
            .matchmaking
            .lock()
            .await
            .get(&session_id)
            .map(|entry| MatchmakingQueueEntry {
                queued_at: entry.queued_at,
                criteria: entry.criteria,
            }))
    }

    async fn ranked_rating(&self, account_id: Uuid) -> Result<RankedRating, GameError> {
        if !self.accounts.contains_key(&account_id) {
            return Err(GameError::Unauthorized);
        }
        Ok(*self
            .ranked_ratings
            .entry(account_id)
            .or_insert(RankedRating {
                rating: DEFAULT_RANKED_RATING,
                matches_played: 0,
            }))
    }

    async fn ranked_profile(
        &self,
        account_id: Uuid,
        season_id: &str,
        season_starts_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<RankedProfile, GameError> {
        if !self.accounts.contains_key(&account_id) {
            return Err(GameError::Unauthorized);
        }
        let _guard = self.ranked_mutations.lock().await;
        self.issue_prior_season_rewards(account_id, season_id, season_starts_at, now);
        let key = (account_id, season_id.to_string());
        let mut standing = self.standing_or_seed(account_id, season_id);
        standing.apply_inactivity_decay(now);
        self.ranked_standings.insert(key, standing.clone());
        self.ranked_ratings.insert(
            account_id,
            RankedRating {
                rating: standing.rating,
                matches_played: standing.matches_played,
            },
        );
        self.ranked_rating_seasons
            .insert(account_id, season_id.to_string());
        let reward_xp_earned = self
            .ranked_rewards
            .iter()
            .filter(|reward| reward.key().0 == account_id)
            .map(|reward| u64::from(*reward.value()))
            .sum();
        Ok(RankedProfile::from_record(&standing, reward_xp_earned))
    }

    async fn ranked_leaderboard_visibility(&self, account_id: Uuid) -> Result<bool, GameError> {
        if !self.accounts.contains_key(&account_id) {
            return Err(GameError::Unauthorized);
        }
        Ok(self
            .leaderboard_visibility
            .get(&account_id)
            .is_none_or(|visible| *visible))
    }

    async fn set_ranked_leaderboard_visibility(
        &self,
        account_id: Uuid,
        visible: bool,
    ) -> Result<(), GameError> {
        if !self.accounts.contains_key(&account_id) {
            return Err(GameError::Unauthorized);
        }
        self.leaderboard_visibility.insert(account_id, visible);
        Ok(())
    }

    async fn ranked_leaderboard(
        &self,
        season_id: &str,
        active_season_id: &str,
        archived: bool,
        cursor: Option<Uuid>,
        limit: usize,
        now: DateTime<Utc>,
    ) -> Result<RankedLeaderboardPage, GameError> {
        let limit = limit.clamp(1, RANKED_LEADERBOARD_MAX_LIMIT);
        let _guard = self.leaderboard_mutations.lock().await;
        if cursor.is_none()
            && season_id != active_season_id
            && !self
                .ranked_standings
                .iter()
                .any(|standing| standing.key().1 == season_id)
            && !self.leaderboard_archives.contains_key(season_id)
        {
            return Err(GameError::InvalidRequest);
        }

        let expired_cursors: Vec<_> = self
            .leaderboard_cursors
            .iter()
            .filter(|cursor| cursor.expires_at <= now)
            .map(|cursor| *cursor.key())
            .collect();
        for cursor_id in expired_cursors {
            self.leaderboard_cursors.remove(&cursor_id);
        }
        let expired_snapshots: Vec<_> = self
            .leaderboard_snapshots
            .iter()
            .filter(|snapshot| {
                snapshot
                    .expires_at
                    .is_some_and(|expires_at| expires_at <= now)
            })
            .map(|snapshot| *snapshot.key())
            .collect();
        for snapshot_id in expired_snapshots {
            self.leaderboard_snapshots.remove(&snapshot_id);
        }

        let (snapshot, after_rank) = if let Some(cursor_id) = cursor {
            let cursor = *self
                .leaderboard_cursors
                .get(&cursor_id)
                .ok_or(GameError::InvalidRequest)?;
            if cursor.expires_at <= now {
                return Err(GameError::InvalidRequest);
            }
            let snapshot = self
                .leaderboard_snapshots
                .get(&cursor.snapshot_id)
                .map(|snapshot| snapshot.clone())
                .ok_or(GameError::InvalidRequest)?;
            if snapshot.season_id != season_id
                || snapshot
                    .expires_at
                    .is_some_and(|expires_at| expires_at <= now)
            {
                return Err(GameError::InvalidRequest);
            }
            (snapshot, cursor.after_rank)
        } else {
            let snapshot = if archived {
                self.leaderboard_archives
                    .get(season_id)
                    .and_then(|snapshot_id| {
                        self.leaderboard_snapshots
                            .get(snapshot_id.value())
                            .map(|snapshot| snapshot.clone())
                    })
                    .unwrap_or_else(|| {
                        let snapshot = self.build_leaderboard_snapshot(season_id, true, now);
                        self.leaderboard_archives
                            .insert(season_id.to_string(), snapshot.id);
                        self.leaderboard_snapshots
                            .insert(snapshot.id, snapshot.clone());
                        snapshot
                    })
            } else {
                self.leaderboard_snapshots
                    .iter()
                    .filter(|snapshot| {
                        snapshot.season_id == season_id
                            && !snapshot.archived
                            && snapshot
                                .expires_at
                                .is_some_and(|expires_at| expires_at > now)
                    })
                    .max_by_key(|snapshot| snapshot.generated_at)
                    .map(|snapshot| snapshot.clone())
                    .unwrap_or_else(|| {
                        let snapshot = self.build_leaderboard_snapshot(season_id, false, now);
                        self.leaderboard_snapshots
                            .insert(snapshot.id, snapshot.clone());
                        snapshot
                    })
            };
            (snapshot, 0)
        };

        let mut eligible: Vec<_> = snapshot
            .entries
            .iter()
            .filter(|entry| entry.rank > after_rank)
            .filter_map(|entry| {
                let account = self.accounts.get(&entry.account_id)?;
                if self
                    .leaderboard_visibility
                    .get(&entry.account_id)
                    .is_some_and(|visible| !*visible)
                    || self.has_leaderboard_penalty(entry.account_id, now)
                {
                    return None;
                }
                Some(RankedLeaderboardEntry {
                    rank: entry.rank,
                    handle: account.0.handle.clone(),
                    rating: entry.rating,
                    tier: RankedTier::for_standing(entry.rating, entry.matches_played),
                    matches_played: entry.matches_played,
                    wins: entry.wins,
                    losses: entry.losses,
                    peak_rating: entry.peak_rating,
                })
            })
            .take(limit.saturating_add(1))
            .collect();
        let has_more = eligible.len() > limit;
        if has_more {
            eligible.truncate(limit);
        }
        let next_cursor = if has_more {
            let after_rank = eligible
                .last()
                .map(|entry| entry.rank)
                .ok_or(GameError::Internal)?;
            let cursor_id = Uuid::new_v4();
            let expires_at = snapshot.expires_at.unwrap_or(now + Duration::minutes(15));
            self.leaderboard_cursors.insert(
                cursor_id,
                LeaderboardCursor {
                    snapshot_id: snapshot.id,
                    after_rank,
                    expires_at,
                },
            );
            Some(cursor_id)
        } else {
            None
        };

        let mut season_ids: HashSet<_> = self
            .ranked_standings
            .iter()
            .map(|standing| standing.key().1.clone())
            .collect();
        season_ids.insert(active_season_id.to_string());
        let mut available_seasons: Vec<_> = season_ids
            .into_iter()
            .map(|available_season_id| RankedLeaderboardSeason {
                archived: available_season_id != active_season_id,
                season_id: available_season_id,
            })
            .collect();
        available_seasons.sort_by(|left, right| {
            left.archived
                .cmp(&right.archived)
                .then_with(|| right.season_id.cmp(&left.season_id))
        });

        Ok(RankedLeaderboardPage {
            season_id: snapshot.season_id,
            archived: snapshot.archived,
            generated_at: snapshot.generated_at,
            entries: eligible,
            next_cursor,
            available_seasons,
        })
    }

    async fn matchmaking_queue_stats(&self) -> Result<MatchmakingQueueStats, GameError> {
        let queue = self.matchmaking.lock().await;
        let oldest_age_seconds = queue
            .values()
            .map(|entry| {
                Utc::now()
                    .signed_duration_since(entry.queued_at)
                    .num_seconds()
                    .max(0) as u64
            })
            .max()
            .unwrap_or_default();
        Ok(MatchmakingQueueStats {
            queued: queue.len() as u64,
            ranked_queued: queue
                .values()
                .filter(|entry| entry.criteria.pool == MatchmakingPool::Ranked)
                .count() as u64,
            oldest_age_seconds,
        })
    }

    async fn prune_expired_data(
        &self,
        inactive_session_before: DateTime<Utc>,
        completed_room_before: DateTime<Utc>,
        abandoned_matchmaking_before: DateTime<Utc>,
        closed_moderation_before: DateTime<Utc>,
        integrity_signal_before: DateTime<Utc>,
    ) -> Result<RetentionStats, GameError> {
        let expired_sessions: Vec<_> = self
            .sessions_by_hash
            .iter()
            .filter(|session| {
                session.current_room_id.is_none() && session.last_seen_at < inactive_session_before
            })
            .map(|session| (session.id, session.token_hash.clone()))
            .collect();
        for (session_id, token_hash) in &expired_sessions {
            self.session_hash_by_id.remove(session_id);
            self.sessions_by_hash.remove(token_hash);
        }

        let expired_rooms: Vec<_> = self
            .rooms
            .iter()
            .filter(|room| {
                matches!(room.status, RoomStatus::Finished | RoomStatus::Cancelled)
                    && room.updated_at < completed_room_before
            })
            .map(|room| room.id)
            .collect();
        for room_id in &expired_rooms {
            self.rooms.remove(room_id);
        }

        let _moderation_guard = self.moderation_mutations.lock().await;
        let expired_report_ids: Vec<_> = self
            .player_reports
            .iter()
            .filter(|report| {
                matches!(
                    report.status,
                    ReportStatus::Actioned | ReportStatus::Dismissed
                ) && report.updated_at < closed_moderation_before
            })
            .map(|report| report.id)
            .collect();
        let expired_action_ids: Vec<_> = self
            .moderation_actions
            .iter()
            .filter(|action| expired_report_ids.contains(&action.report_id))
            .map(|action| action.id)
            .collect();
        for action_id in expired_action_ids {
            self.moderation_actions.remove(&action_id);
        }
        for report_id in &expired_report_ids {
            self.player_reports.remove(report_id);
        }

        let _integrity_guard = self.integrity_mutations.lock().await;
        let expired_signal_ids: Vec<_> = self
            .integrity_signals
            .iter()
            .filter(|signal| signal.last_observed_at < integrity_signal_before)
            .map(|signal| signal.id)
            .collect();
        for signal_id in &expired_signal_ids {
            self.integrity_signals.remove(signal_id);
        }

        let mut queue = self.matchmaking.lock().await;
        let queue_before = queue.len();
        queue.retain(|_, entry| entry.queued_at >= abandoned_matchmaking_before);
        Ok(RetentionStats {
            sessions_deleted: expired_sessions.len() as u64,
            rooms_deleted: expired_rooms.len() as u64,
            matchmaking_entries_deleted: queue_before.saturating_sub(queue.len()) as u64,
            moderation_cases_deleted: expired_report_ids.len() as u64,
            integrity_signals_deleted: expired_signal_ids.len() as u64,
        })
    }

    fn kind(&self) -> &'static str {
        "memory"
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{ReportCategory, baseline_live_content};
    use chrono::Utc;

    use super::*;

    fn session() -> UserSession {
        named_session("Alpha")
    }

    fn named_session(nickname: &str) -> UserSession {
        UserSession {
            id: Uuid::new_v4(),
            account_id: None,
            nickname: nickname.to_string(),
            token_hash: Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            last_seen_at: Utc::now(),
            current_room_id: None,
        }
    }

    async fn ranked_session(store: &MemoryStore, nickname: &str) -> UserSession {
        let account_id = Uuid::new_v4();
        let mut session = named_session(nickname);
        session.account_id = Some(account_id);
        store.accounts.insert(
            account_id,
            (
                PlayerAccount {
                    id: account_id,
                    handle: nickname.to_string(),
                    created_at: Utc::now(),
                },
                "ranked-test-recovery-hash".to_string(),
            ),
        );
        store.save_session(&session).await.unwrap();
        store
            .ranked_profile(
                account_id,
                "TEST_SEASON",
                Utc::now() - Duration::days(1),
                Utc::now(),
            )
            .await
            .unwrap();
        session
    }

    fn finished_ranked_room(
        first: &UserSession,
        second: &UserSession,
        season_id: &str,
        finished_at: DateTime<Utc>,
    ) -> GameRoom {
        let mut room = GameRoom::new(
            Uuid::new_v4().simple().to_string()[..6].to_ascii_uppercase(),
            "Ranked result".to_string(),
            RoomVisibility::Private,
            first,
        )
        .unwrap();
        room.join(second).unwrap();
        let winner_id = room.players[0].id;
        let loser_id = room.players[1].id;
        room.status = RoomStatus::Finished;
        room.game_id = Some(Uuid::new_v4());
        room.ranked_match = Some(crate::domain::RankedMatchContext {
            season_id: season_id.to_string(),
            content_revision: 1,
        });
        room.game = Some(crate::domain::Game {
            balance: room.balance.clone(),
            boards: HashMap::new(),
            attacks: Vec::new(),
            timeline: Vec::new(),
            first_player_id: winner_id,
            mode: crate::domain::GameMode::Classic,
            shots_remaining_in_turn: 0,
            current_player_id: winner_id,
            turn_number: 1,
            started_at: finished_at - Duration::minutes(1),
            turn_duration_seconds: 60,
            turn_started_at: None,
            turn_deadline_at: None,
            consecutive_timeout_counts: HashMap::new(),
            total_timeout_counts: HashMap::new(),
            result: Some(crate::domain::GameResult {
                winner_id,
                loser_id,
                total_turns: 1,
                duration_seconds: 60,
                finished_at,
                players: Vec::new(),
                finish_reason: FinishReason::FleetDestroyed,
                win_type: crate::domain::WinType::NormalVictory,
            }),
        });
        room
    }

    #[tokio::test]
    async fn stale_room_snapshots_cannot_overwrite_a_newer_revision() {
        let store = MemoryStore::default();
        let mut room = GameRoom::new(
            "CAS234".to_string(),
            "Original".to_string(),
            RoomVisibility::Private,
            &session(),
        )
        .unwrap();
        store.save_room(&mut room).await.unwrap();
        assert_eq!(room.persistence_revision, 1);

        let mut stale = room.clone();
        room.name = "Authoritative".to_string();
        store.save_room(&mut room).await.unwrap();
        assert_eq!(room.persistence_revision, 2);

        stale.name = "Stale overwrite".to_string();
        assert_eq!(
            store.save_room(&mut stale).await.unwrap_err(),
            GameError::VersionConflict
        );
        assert_eq!(
            store.room_by_id(room.id).await.unwrap().unwrap().name,
            "Authoritative"
        );
    }

    #[tokio::test]
    async fn matchmaking_claims_and_completes_each_pair_exactly_once() {
        let store = MemoryStore::default();
        let first = named_session("Alpha");
        let second = named_session("Bravo");
        store.save_session(&first).await.unwrap();
        store.save_session(&second).await.unwrap();

        let queued = store
            .enqueue_matchmaking(&first, MatchmakingCriteria::casual(first.id))
            .await
            .unwrap();
        assert!(queued.claim.is_none());
        assert_eq!(
            store.matchmaking_entry(first.id).await.unwrap(),
            Some(MatchmakingQueueEntry {
                queued_at: queued.queued_at,
                criteria: MatchmakingCriteria::casual(first.id),
            })
        );

        let matched = store
            .enqueue_matchmaking(&second, MatchmakingCriteria::casual(second.id))
            .await
            .unwrap();
        let claim = matched.claim.unwrap();
        assert_eq!(claim.opponent.id, first.id);
        assert_eq!(claim.opponent_queued_at, queued.queued_at);
        assert!(
            store
                .enqueue_matchmaking(&first, MatchmakingCriteria::casual(first.id))
                .await
                .unwrap()
                .claim
                .is_none()
        );
        assert!(!store.cancel_matchmaking(first.id).await.unwrap());

        let mut room = GameRoom::new(
            "MATCH1".to_string(),
            "Rapid match".to_string(),
            RoomVisibility::Private,
            &claim.opponent,
        )
        .unwrap();
        room.join(&second).unwrap();
        store
            .complete_matchmaking(claim.id, &mut room)
            .await
            .unwrap();

        assert_eq!(room.persistence_revision, 1);
        assert_eq!(
            store
                .session_by_token_hash(&first.token_hash)
                .await
                .unwrap()
                .unwrap()
                .current_room_id,
            Some(room.id)
        );
        assert_eq!(
            store
                .session_by_token_hash(&second.token_hash)
                .await
                .unwrap()
                .unwrap()
                .current_room_id,
            Some(room.id)
        );
        assert!(store.matchmaking_entry(first.id).await.unwrap().is_none());
        assert!(store.matchmaking_entry(second.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn ranked_matchmaking_uses_authoritative_rating_and_mutual_widening() {
        let store = MemoryStore::default();
        let first = ranked_session(&store, "Rank Alpha").await;
        let second = ranked_session(&store, "Rank Bravo").await;
        let first_account = first.account_id.unwrap();
        let second_account = second.account_id.unwrap();
        store.ranked_ratings.insert(
            second_account,
            RankedRating {
                rating: 1_680,
                matches_played: 12,
            },
        );
        let first_criteria = MatchmakingCriteria::ranked(
            first_account,
            crate::domain::MatchmakingRegion::Korea,
            80,
            DEFAULT_RANKED_RATING,
            ranked_season_key("TEST_SEASON"),
        )
        .unwrap();
        let second_criteria = MatchmakingCriteria::ranked(
            second_account,
            crate::domain::MatchmakingRegion::Japan,
            90,
            1_680,
            ranked_season_key("TEST_SEASON"),
        )
        .unwrap();

        assert!(
            store
                .enqueue_matchmaking(&first, first_criteria)
                .await
                .unwrap()
                .claim
                .is_none()
        );
        let changed_profile = MatchmakingCriteria::ranked(
            first_account,
            crate::domain::MatchmakingRegion::Japan,
            80,
            DEFAULT_RANKED_RATING,
            ranked_season_key("TEST_SEASON"),
        )
        .unwrap();
        assert_eq!(
            store
                .enqueue_matchmaking(&first, changed_profile)
                .await
                .unwrap_err(),
            GameError::InvalidState,
            "an idempotent ticket cannot be mutated to reset or widen its profile"
        );
        assert!(
            store
                .enqueue_matchmaking(&second, second_criteria)
                .await
                .unwrap()
                .claim
                .is_none(),
            "different regional/rating exact windows must not match"
        );
        {
            let mut queue = store.matchmaking.lock().await;
            for entry in queue.values_mut() {
                entry.queued_at = Utc::now() - Duration::seconds(31);
            }
        }
        let matched = store
            .enqueue_matchmaking(&second, second_criteria)
            .await
            .unwrap()
            .claim
            .expect("both widened regional windows should match");
        assert_eq!(matched.opponent.id, first.id);
        assert_eq!(matched.quality.rating_delta, 180);
        assert_eq!(
            matched.quality.phase,
            crate::domain::MatchmakingSearchPhase::Regional
        );
        assert_eq!(
            store.matchmaking_queue_stats().await.unwrap().ranked_queued,
            2
        );
    }

    #[tokio::test]
    async fn ranked_matchmaking_rejects_guests_spoofed_ratings_and_same_party_sessions() {
        let store = MemoryStore::default();
        let guest = named_session("Rank Guest");
        store.save_session(&guest).await.unwrap();
        let fake_account = Uuid::new_v4();
        let guest_criteria = MatchmakingCriteria::ranked(
            fake_account,
            crate::domain::MatchmakingRegion::Korea,
            40,
            DEFAULT_RANKED_RATING,
            ranked_season_key("TEST_SEASON"),
        )
        .unwrap();
        assert_eq!(
            store
                .enqueue_matchmaking(&guest, guest_criteria)
                .await
                .unwrap_err(),
            GameError::Unauthorized
        );

        let first = ranked_session(&store, "Party Alpha").await;
        let account_id = first.account_id.unwrap();
        let spoofed = MatchmakingCriteria::ranked(
            account_id,
            crate::domain::MatchmakingRegion::Korea,
            40,
            DEFAULT_RANKED_RATING + 500,
            ranked_season_key("TEST_SEASON"),
        )
        .unwrap();
        assert_eq!(
            store
                .enqueue_matchmaking(&first, spoofed)
                .await
                .unwrap_err(),
            GameError::InvalidRequest
        );

        let criteria = MatchmakingCriteria::ranked(
            account_id,
            crate::domain::MatchmakingRegion::Korea,
            40,
            DEFAULT_RANKED_RATING,
            ranked_season_key("TEST_SEASON"),
        )
        .unwrap();
        assert!(
            store
                .enqueue_matchmaking(&first, criteria)
                .await
                .unwrap()
                .claim
                .is_none()
        );
        let mut second = named_session("Party Bravo");
        second.account_id = Some(account_id);
        store.save_session(&second).await.unwrap();
        assert!(
            store
                .enqueue_matchmaking(&second, criteria)
                .await
                .unwrap()
                .claim
                .is_none(),
            "two sessions owned by one account are the same solo party"
        );
    }

    #[tokio::test]
    async fn ranked_matchmaking_avoids_recent_opponents_then_relaxes_without_starving_them() {
        let store = MemoryStore::default();
        let first = ranked_session(&store, "Fair Alpha").await;
        let recent = ranked_session(&store, "Fair Bravo").await;
        let novel = ranked_session(&store, "Fair Charlie").await;
        let first_account = first.account_id.unwrap();
        let recent_account = recent.account_id.unwrap();
        let novel_account = novel.account_id.unwrap();
        let now = Utc::now();
        let mut previous = finished_ranked_room(&first, &recent, "TEST_SEASON", now);
        store.save_room(&mut previous).await.unwrap();

        let criteria = |session: &UserSession, rating: i32| {
            MatchmakingCriteria::ranked(
                session.account_id.unwrap(),
                crate::domain::MatchmakingRegion::Korea,
                55,
                rating,
                ranked_season_key("TEST_SEASON"),
            )
            .unwrap()
        };
        let first_criteria = criteria(
            &first,
            store.ranked_rating(first_account).await.unwrap().rating,
        );
        let recent_criteria = criteria(
            &recent,
            store.ranked_rating(recent_account).await.unwrap().rating,
        );
        let novel_criteria = criteria(
            &novel,
            store.ranked_rating(novel_account).await.unwrap().rating,
        );

        {
            let mut queue = store.matchmaking.lock().await;
            queue.insert(
                recent.id,
                MatchmakingEntry {
                    session: recent.clone(),
                    queued_at: now - Duration::seconds(100),
                    criteria: recent_criteria,
                    claim_id: None,
                    claimed_at: None,
                },
            );
        }
        assert!(
            store
                .enqueue_matchmaking(&first, first_criteria)
                .await
                .unwrap()
                .claim
                .is_none(),
            "a recent opponent must be excluded before both tickets reach global search"
        );
        {
            let mut queue = store.matchmaking.lock().await;
            for entry in queue.values_mut() {
                entry.queued_at = now - Duration::seconds(100);
            }
            queue.insert(
                novel.id,
                MatchmakingEntry {
                    session: novel.clone(),
                    queued_at: now - Duration::seconds(91),
                    criteria: novel_criteria,
                    claim_id: None,
                    claimed_at: None,
                },
            );
        }
        let novel_match = store
            .enqueue_matchmaking(&first, first_criteria)
            .await
            .unwrap()
            .claim
            .expect("a novel opponent must outrank an older eligible rematch");
        assert_eq!(novel_match.opponent.id, novel.id);
        assert_eq!(novel_match.quality.recent_pairings, 0);
        assert!(!novel_match.quality.rematch_relaxed);
        store
            .release_matchmaking_claim(novel_match.id)
            .await
            .unwrap();
        assert!(store.cancel_matchmaking(first.id).await.unwrap());
        assert!(store.cancel_matchmaking(novel.id).await.unwrap());

        assert!(
            store
                .enqueue_matchmaking(&first, first_criteria)
                .await
                .unwrap()
                .claim
                .is_none()
        );
        {
            let mut queue = store.matchmaking.lock().await;
            for entry in queue.values_mut() {
                entry.queued_at = Utc::now() - Duration::seconds(91);
            }
        }
        let relaxed = store
            .enqueue_matchmaking(&first, first_criteria)
            .await
            .unwrap()
            .claim
            .expect("mutual global wait must eventually permit the only recent opponent");
        assert_eq!(relaxed.opponent.id, recent.id);
        assert_eq!(relaxed.quality.recent_pairings, 1);
        assert!(relaxed.quality.rematch_relaxed);
        assert!(relaxed.quality.shared_wait_seconds >= 90);
    }

    #[tokio::test]
    async fn ranked_leaderboard_snapshots_filter_penalties_and_honor_privacy_immediately() {
        let store = MemoryStore::default();
        let first = ranked_session(&store, "Board Alpha").await;
        let second = ranked_session(&store, "Board Bravo").await;
        let third = ranked_session(&store, "Board Charlie").await;
        let now = Utc::now();
        let standing = |rating, wins| RankedStandingRecord {
            season_id: "LEADERBOARD_S1".to_string(),
            rating,
            matches_played: 10,
            wins,
            losses: 10 - wins,
            peak_rating: rating + 50,
            last_match_at: Some(now),
            decay_steps_applied: 0,
            season_reward_issued_at: None,
        };
        store.ranked_standings.insert(
            (first.account_id.unwrap(), "LEADERBOARD_S1".to_string()),
            standing(1_900, 8),
        );
        store.ranked_standings.insert(
            (second.account_id.unwrap(), "LEADERBOARD_S1".to_string()),
            standing(1_800, 7),
        );
        store.ranked_standings.insert(
            (third.account_id.unwrap(), "LEADERBOARD_S1".to_string()),
            standing(1_700, 6),
        );

        let first_page = store
            .ranked_leaderboard("LEADERBOARD_S1", "LEADERBOARD_S1", false, None, 1, now)
            .await
            .unwrap();
        assert_eq!(first_page.entries[0].handle, "Board Alpha");
        assert_eq!(first_page.entries[0].rank, 1);
        let cursor = first_page.next_cursor.expect("a second page must exist");
        assert!(
            !serde_json::to_value(&first_page)
                .unwrap()
                .to_string()
                .contains("accountId")
        );

        let suspension_id = Uuid::new_v4();
        store.moderation_actions.insert(
            suspension_id,
            ModerationAction {
                id: suspension_id,
                report_id: Uuid::new_v4(),
                target_identity_id: second.account_id.unwrap(),
                operator_id: "leaderboard-test".to_string(),
                action: ModerationActionKind::Suspend,
                reason: "competitive integrity review".to_string(),
                expires_at: Some(now + Duration::hours(1)),
                reverses_action_id: None,
                created_at: now,
            },
        );
        let second_page = store
            .ranked_leaderboard(
                "LEADERBOARD_S1",
                "LEADERBOARD_S1",
                false,
                Some(cursor),
                1,
                now,
            )
            .await
            .unwrap();
        assert_eq!(second_page.entries[0].handle, "Board Charlie");
        assert_eq!(second_page.entries[0].rank, 3);

        store
            .set_ranked_leaderboard_visibility(first.account_id.unwrap(), false)
            .await
            .unwrap();
        let private_page = store
            .ranked_leaderboard("LEADERBOARD_S1", "LEADERBOARD_S1", false, None, 10, now)
            .await
            .unwrap();
        assert_eq!(private_page.entries.len(), 1);
        assert_eq!(private_page.entries[0].handle, "Board Charlie");

        let archived = store
            .ranked_leaderboard("LEADERBOARD_S1", "LEADERBOARD_S2", true, None, 10, now)
            .await
            .unwrap();
        store
            .ranked_standings
            .get_mut(&(third.account_id.unwrap(), "LEADERBOARD_S1".to_string()))
            .unwrap()
            .rating = 2_500;
        let archived_again = store
            .ranked_leaderboard(
                "LEADERBOARD_S1",
                "LEADERBOARD_S2",
                true,
                None,
                10,
                now + Duration::minutes(1),
            )
            .await
            .unwrap();
        assert!(archived.archived);
        assert_eq!(archived.entries, archived_again.entries);
        assert!(
            archived_again
                .available_seasons
                .iter()
                .any(|season| season.season_id == "LEADERBOARD_S2" && !season.archived)
        );
    }

    #[tokio::test]
    async fn ranked_results_update_rating_and_rewards_exactly_once() {
        let store = MemoryStore::default();
        let first = ranked_session(&store, "Settle Alpha").await;
        let second = ranked_session(&store, "Settle Bravo").await;
        let first_account = first.account_id.unwrap();
        let second_account = second.account_id.unwrap();
        let finished_at = Utc::now();
        let mut room = finished_ranked_room(&first, &second, "TEST_SEASON", finished_at);

        store.save_room(&mut room).await.unwrap();
        let winner = store
            .ranked_profile(
                first_account,
                "TEST_SEASON",
                Utc::now() - Duration::days(1),
                Utc::now(),
            )
            .await
            .unwrap();
        let loser = store
            .ranked_profile(
                second_account,
                "TEST_SEASON",
                Utc::now() - Duration::days(1),
                Utc::now(),
            )
            .await
            .unwrap();
        assert_eq!((winner.rating, winner.reward_xp_earned), (1_532, 100));
        assert_eq!((loser.rating, loser.reward_xp_earned), (1_468, 40));

        store.save_room(&mut room).await.unwrap();
        assert_eq!(
            store
                .ranked_profile(
                    first_account,
                    "TEST_SEASON",
                    Utc::now() - Duration::days(1),
                    Utc::now(),
                )
                .await
                .unwrap()
                .matches_played,
            1,
            "saving a finished room again must not apply a second rating change"
        );
    }

    #[tokio::test]
    async fn retention_prunes_only_expired_inactive_data() {
        let store = MemoryStore::default();
        let now = Utc::now();
        let mut expired = named_session("Expired");
        expired.last_seen_at = now - Duration::days(2);
        let active = named_session("Active");
        store.save_session(&expired).await.unwrap();
        store.save_session(&active).await.unwrap();
        store
            .enqueue_matchmaking(&active, MatchmakingCriteria::casual(active.id))
            .await
            .unwrap();

        let mut cancelled = GameRoom::new(
            "OLD234".to_string(),
            "Expired operation".to_string(),
            RoomVisibility::Private,
            &expired,
        )
        .unwrap();
        cancelled.leave(expired.id).unwrap();
        cancelled.updated_at = now - Duration::days(100);
        store.save_room(&mut cancelled).await.unwrap();

        let expired_report = NewPlayerReport {
            id: Uuid::new_v4(),
            reporter_identity_id: expired.id,
            target_identity_id: active.id,
            room_id: cancelled.id,
            target_player_id: Uuid::new_v4(),
            target_nickname: "Active".to_string(),
            category: ReportCategory::Other,
            details: "expired moderation fixture".to_string(),
            evidence: serde_json::json!({"fixture": true}),
            created_at: now - Duration::days(400),
        };
        store.create_player_report(&expired_report).await.unwrap();
        store
            .apply_moderation_action(&NewModerationAction {
                id: Uuid::new_v4(),
                report_id: expired_report.id,
                operator_id: "retention-test".to_string(),
                action: ModerationActionKind::Dismiss,
                reason: "closed fixture".to_string(),
                expires_at: None,
                reverses_action_id: None,
                created_at: now - Duration::days(400),
            })
            .await
            .unwrap();
        let recent_report = NewPlayerReport {
            id: Uuid::new_v4(),
            created_at: now,
            ..expired_report.clone()
        };
        store.create_player_report(&recent_report).await.unwrap();

        let expired_signal = NewIntegritySignal {
            id: Uuid::new_v4(),
            subject_identity_id: expired.id,
            room_id: Some(cancelled.id),
            kind: IntegritySignalKind::Automation,
            severity: 2,
            confidence: 0.75,
            evidence: serde_json::json!({"fixture": "expired"}),
            observed_at: now - Duration::days(200),
        };
        store
            .record_integrity_signal(&expired_signal)
            .await
            .unwrap();
        store
            .record_integrity_signal(&NewIntegritySignal {
                id: Uuid::new_v4(),
                subject_identity_id: active.id,
                room_id: None,
                evidence: serde_json::json!({"fixture": "recent"}),
                observed_at: now,
                ..expired_signal.clone()
            })
            .await
            .unwrap();

        let stats = store
            .prune_expired_data(
                now - Duration::days(1),
                now - Duration::days(90),
                now + Duration::seconds(1),
                now - Duration::days(365),
                now - Duration::days(180),
            )
            .await
            .unwrap();
        assert_eq!(stats.sessions_deleted, 1);
        assert_eq!(stats.rooms_deleted, 1);
        assert_eq!(stats.matchmaking_entries_deleted, 1);
        assert_eq!(stats.moderation_cases_deleted, 1);
        assert_eq!(stats.integrity_signals_deleted, 1);
        assert!(!store.player_reports.contains_key(&expired_report.id));
        assert!(store.player_reports.contains_key(&recent_report.id));
        assert!(!store.integrity_signals.contains_key(&expired_signal.id));
        assert_eq!(store.integrity_signals.len(), 1);
        assert!(
            store
                .session_by_token_hash(&expired.token_hash)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .session_by_token_hash(&active.token_hash)
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn mission_reward_ledger_is_idempotent_per_account_source_and_period() {
        let store = MemoryStore::default();
        let account_id = Uuid::new_v4();
        assert!(
            store
                .claim_mission_reward(account_id, "DAILY_DEPLOYMENT", "2026-08-15", 100)
                .await
                .unwrap()
        );
        assert!(
            !store
                .claim_mission_reward(account_id, "DAILY_DEPLOYMENT", "2026-08-15", 100)
                .await
                .unwrap()
        );
        assert!(
            store
                .claim_mission_reward(account_id, "DAILY_DEPLOYMENT", "2026-08-16", 100)
                .await
                .unwrap()
        );
        let rewards = store.mission_rewards(account_id).await.unwrap();
        assert_eq!(rewards.len(), 2);
        assert_eq!(rewards.iter().map(|reward| reward.xp).sum::<u32>(), 200);
    }

    #[tokio::test]
    async fn live_content_commits_with_cas_and_activates_only_eligible_revisions() {
        let store = MemoryStore::default();
        let now = Utc::now();
        let baseline = baseline_live_content();
        let scheduled = LiveContentRevision::from_payload(
            1,
            baseline.payload_for_rollback(now + Duration::hours(1), "Schedule revision one".into()),
            "test-operator".into(),
            now,
            None,
        );
        assert!(store.commit_live_content(0, &scheduled).await.unwrap());
        assert!(!store.commit_live_content(0, &scheduled).await.unwrap());
        assert!(store.active_live_content(now).await.unwrap().is_none());

        let immediate = LiveContentRevision::from_payload(
            2,
            baseline.payload_for_rollback(now, "Publish immediate revision two".into()),
            "test-operator".into(),
            now,
            None,
        );
        assert!(store.commit_live_content(1, &immediate).await.unwrap());
        assert_eq!(
            store
                .active_live_content(now)
                .await
                .unwrap()
                .unwrap()
                .revision,
            2
        );
        assert_eq!(
            store
                .live_content_history(10)
                .await
                .unwrap()
                .iter()
                .map(|revision| revision.revision)
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
    }

    #[tokio::test]
    async fn integrity_signals_deduplicate_room_evidence_and_remain_searchable() {
        let store = MemoryStore::default();
        let subject_identity_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let first = store
            .record_integrity_signal(&NewIntegritySignal {
                id: Uuid::new_v4(),
                subject_identity_id,
                room_id: Some(room_id),
                kind: IntegritySignalKind::ImpossibleOrder,
                severity: 2,
                confidence: 0.72,
                evidence: serde_json::json!({ "errorCode": "NOT_YOUR_TURN" }),
                observed_at: Utc::now(),
            })
            .await
            .unwrap();
        let repeated = store
            .record_integrity_signal(&NewIntegritySignal {
                id: Uuid::new_v4(),
                subject_identity_id,
                room_id: Some(room_id),
                kind: IntegritySignalKind::ImpossibleOrder,
                severity: 4,
                confidence: 0.96,
                evidence: serde_json::json!({ "errorCode": "UNAUTHORIZED" }),
                observed_at: Utc::now(),
            })
            .await
            .unwrap();
        assert_eq!(first.id, repeated.id);
        assert_eq!(repeated.occurrences, 2);
        assert_eq!(repeated.severity, 4);
        assert_eq!(repeated.confidence, 0.96);

        let page = store
            .integrity_signals(
                Some("unauthorized"),
                Some(IntegritySignalKind::ImpossibleOrder),
                None,
                25,
            )
            .await
            .unwrap();
        assert_eq!(page.signals.len(), 1);
        assert_eq!(page.signals[0].subject_identity_id, subject_identity_id);
        assert!(page.next_before.is_none());
    }
}
