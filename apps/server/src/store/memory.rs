use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use std::collections::HashMap;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    domain::{
        AccountSession, ActivePenalty, FinishReason, GameRoom, IntegritySignal,
        IntegritySignalKind, IntegritySignalPage, ModerationAction, ModerationActionKind,
        ModerationCase, ModerationCasePage, NewIntegritySignal, NewModerationAction,
        NewPlayerReport, PlayerAccount, PlayerReport, ReportStatus, RoomStatus, RoomSummary,
        RoomVisibility, SocialRelationship, UserSession,
    },
    error::GameError,
};

use super::{
    GameHistoryItem, GameStore, MatchmakingClaim, MatchmakingEnqueueResult, MatchmakingQueueStats,
    MissionReward, RetentionStats,
};

#[derive(Debug, Clone)]
struct MatchmakingEntry {
    session: UserSession,
    queued_at: DateTime<Utc>,
    claim_id: Option<Uuid>,
    claimed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default)]
pub struct MemoryStore {
    sessions_by_hash: DashMap<String, UserSession>,
    session_hash_by_id: DashMap<Uuid, String>,
    accounts: DashMap<Uuid, (PlayerAccount, String)>,
    account_id_by_handle: DashMap<String, Uuid>,
    account_mutations: Mutex<()>,
    mission_rewards: DashMap<(Uuid, String, String), u32>,
    social_relationships: DashMap<(Uuid, Uuid), SocialRelationship>,
    player_reports: DashMap<Uuid, PlayerReport>,
    moderation_actions: DashMap<Uuid, ModerationAction>,
    moderation_mutations: Mutex<()>,
    integrity_signals: DashMap<Uuid, IntegritySignal>,
    integrity_mutations: Mutex<()>,
    rooms: DashMap<Uuid, GameRoom>,
    matchmaking: Mutex<HashMap<Uuid, MatchmakingEntry>>,
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

    async fn identity_for_session(&self, session_id: Uuid) -> Result<Option<Uuid>, GameError> {
        Ok(self.session_hash_by_id.get(&session_id).and_then(|hash| {
            self.sessions_by_hash
                .get(hash.value())
                .map(|session| session.account_id.unwrap_or(session.id))
        }))
    }

    async fn set_social_relationship(
        &self,
        actor_identity_id: Uuid,
        relationship: SocialRelationship,
    ) -> Result<(), GameError> {
        let key = (actor_identity_id, relationship.target_identity_id);
        if relationship.muted || relationship.blocked {
            self.social_relationships.insert(key, relationship);
        } else {
            self.social_relationships.remove(&key);
        }
        Ok(())
    }

    async fn social_relationships(
        &self,
        actor_identity_id: Uuid,
    ) -> Result<Vec<SocialRelationship>, GameError> {
        let mut relationships: Vec<_> = self
            .social_relationships
            .iter()
            .filter(|relationship| relationship.key().0 == actor_identity_id)
            .map(|relationship| relationship.value().clone())
            .collect();
        relationships.sort_by_key(|relationship| std::cmp::Reverse(relationship.updated_at));
        Ok(relationships)
    }

    async fn social_relationship_between(
        &self,
        actor_identity_id: Uuid,
        target_identity_id: Uuid,
    ) -> Result<Option<SocialRelationship>, GameError> {
        Ok(self
            .social_relationships
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
            if let Some(existing_id) = self.integrity_signals.iter().find_map(|entry| {
                (entry.subject_identity_id == signal.subject_identity_id
                    && entry.room_id == Some(room_id)
                    && entry.kind == signal.kind)
                    .then_some(entry.id)
            }) {
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
        let identity_for_session = |session_id: Uuid| {
            self.session_hash_by_id
                .get(&session_id)
                .and_then(|hash| self.sessions_by_hash.get(hash.value()))
                .map(|session| session.account_id.unwrap_or(session.id))
        };
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
                    .filter_map(|player| identity_for_session(player.session_id))
                    .collect();
                identities.contains(&first_identity_id) && identities.contains(&second_identity_id)
            })
            .count() as u64)
    }

    async fn save_room(&self, room: &mut GameRoom) -> Result<(), GameError> {
        if self
            .rooms
            .get(&room.id)
            .is_some_and(|stored| stored.persistence_revision != room.persistence_revision)
        {
            return Err(GameError::VersionConflict);
        }
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
    ) -> Result<MatchmakingEnqueueResult, GameError> {
        let stored_session = self
            .sessions_by_hash
            .get(&session.token_hash)
            .ok_or(GameError::Unauthorized)?;
        if stored_session.current_room_id.is_some() {
            return Err(GameError::AlreadyJoined);
        }
        drop(stored_session);

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
            if entry.claim_id.is_some() {
                return Ok(MatchmakingEnqueueResult {
                    queued_at: entry.queued_at,
                    claim: None,
                });
            }
            entry.queued_at
        } else {
            queue.insert(
                session.id,
                MatchmakingEntry {
                    session: session.clone(),
                    queued_at: now,
                    claim_id: None,
                    claimed_at: None,
                },
            );
            now
        };

        let own_identity = session.account_id.unwrap_or(session.id);
        let opponent_id = queue
            .iter()
            .filter(|(session_id, entry)| {
                if **session_id == session.id || entry.claim_id.is_some() {
                    return false;
                }
                let opponent_identity = entry.session.account_id.unwrap_or(entry.session.id);
                let own_blocks = self
                    .social_relationships
                    .get(&(own_identity, opponent_identity))
                    .is_some_and(|relationship| relationship.blocked);
                let opponent_blocks = self
                    .social_relationships
                    .get(&(opponent_identity, own_identity))
                    .is_some_and(|relationship| relationship.blocked);
                !own_blocks && !opponent_blocks
            })
            .min_by_key(|(_, entry)| entry.queued_at)
            .map(|(session_id, _)| *session_id);
        let Some(opponent_id) = opponent_id else {
            return Ok(MatchmakingEnqueueResult {
                queued_at,
                claim: None,
            });
        };

        let claim_id = Uuid::new_v4();
        let opponent = queue
            .get_mut(&opponent_id)
            .expect("selected matchmaking opponent must exist");
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
            claim: Some(MatchmakingClaim {
                id: claim_id,
                opponent,
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

    async fn matchmaking_time(&self, session_id: Uuid) -> Result<Option<DateTime<Utc>>, GameError> {
        Ok(self
            .matchmaking
            .lock()
            .await
            .get(&session_id)
            .map(|entry| entry.queued_at))
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
            oldest_age_seconds,
        })
    }

    async fn prune_expired_data(
        &self,
        inactive_session_before: DateTime<Utc>,
        completed_room_before: DateTime<Utc>,
        abandoned_matchmaking_before: DateTime<Utc>,
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

        let mut queue = self.matchmaking.lock().await;
        let queue_before = queue.len();
        queue.retain(|_, entry| entry.queued_at >= abandoned_matchmaking_before);
        Ok(RetentionStats {
            sessions_deleted: expired_sessions.len() as u64,
            rooms_deleted: expired_rooms.len() as u64,
            matchmaking_entries_deleted: queue_before.saturating_sub(queue.len()) as u64,
        })
    }

    fn kind(&self) -> &'static str {
        "memory"
    }
}

#[cfg(test)]
mod tests {
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

        let queued = store.enqueue_matchmaking(&first).await.unwrap();
        assert!(queued.claim.is_none());
        assert_eq!(
            store.matchmaking_time(first.id).await.unwrap(),
            Some(queued.queued_at)
        );

        let matched = store.enqueue_matchmaking(&second).await.unwrap();
        let claim = matched.claim.unwrap();
        assert_eq!(claim.opponent.id, first.id);
        assert!(
            store
                .enqueue_matchmaking(&first)
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
        assert!(store.matchmaking_time(first.id).await.unwrap().is_none());
        assert!(store.matchmaking_time(second.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn retention_prunes_only_expired_inactive_data() {
        let store = MemoryStore::default();
        let mut expired = named_session("Expired");
        expired.last_seen_at = Utc::now() - Duration::days(2);
        let active = named_session("Active");
        store.save_session(&expired).await.unwrap();
        store.save_session(&active).await.unwrap();
        store.enqueue_matchmaking(&active).await.unwrap();

        let mut cancelled = GameRoom::new(
            "OLD234".to_string(),
            "Expired operation".to_string(),
            RoomVisibility::Private,
            &expired,
        )
        .unwrap();
        cancelled.leave(expired.id).unwrap();
        cancelled.updated_at = Utc::now() - Duration::days(100);
        store.save_room(&mut cancelled).await.unwrap();

        let stats = store
            .prune_expired_data(
                Utc::now() - Duration::days(1),
                Utc::now() - Duration::days(90),
                Utc::now() + Duration::seconds(1),
            )
            .await
            .unwrap();
        assert_eq!(stats.sessions_deleted, 1);
        assert_eq!(stats.rooms_deleted, 1);
        assert_eq!(stats.matchmaking_entries_deleted, 1);
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
}
