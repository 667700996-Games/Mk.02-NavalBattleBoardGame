use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use redis::{AsyncCommands, aio::ConnectionManager};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::{
    domain::{
        AccountSession, ActivePenalty, ChatMessageType, GameRoom, IntegritySignal,
        IntegritySignalKind, IntegritySignalPage, ModerationAction, ModerationActionKind,
        ModerationCase, ModerationCasePage, NewIntegritySignal, NewModerationAction,
        NewPlayerReport, PlayerAccount, PlayerReport, ReportCategory, ReportStatus, RoomStatus,
        RoomSummary, SocialRelationship, UserSession,
    },
    error::GameError,
};

use super::{
    AccountDeletionStats, GameHistoryItem, GameStore, MatchmakingClaim, MatchmakingEnqueueResult,
    MatchmakingQueueStats, MissionReward, RetentionStats, RoomAuthorityLease,
};

#[derive(Clone)]
pub struct PostgresRedisStore {
    pool: PgPool,
    cache: Option<ConnectionManager>,
}

impl std::fmt::Debug for PostgresRedisStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PostgresRedisStore")
            .finish_non_exhaustive()
    }
}

impl PostgresRedisStore {
    pub async fn connect(database_url: &str, redis_url: &str) -> Result<Self, GameError> {
        let pool = PgPoolOptions::new()
            .max_connections(16)
            .min_connections(1)
            .connect(database_url)
            .await?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|error| {
                tracing::error!(%error, "database migration failed");
                GameError::StorageUnavailable
            })?;
        let cache = match redis::Client::open(redis_url) {
            Ok(client) => match ConnectionManager::new(client).await {
                Ok(cache) => Some(cache),
                Err(error) => {
                    tracing::warn!(%error, "redis unavailable; continuing with postgres only");
                    None
                }
            },
            Err(error) => {
                tracing::warn!(%error, "redis configuration invalid; continuing with postgres only");
                None
            }
        };
        Ok(Self { pool, cache })
    }

    fn room_cache_key(id: Uuid) -> String {
        format!("mk01:room:{id}")
    }

    async fn cache_room(&self, room: &GameRoom) -> Result<(), GameError> {
        let Some(mut cache) = self.cache.clone() else {
            return Ok(());
        };
        let data = serde_json::to_string(room).map_err(|_| GameError::Internal)?;
        if let Err(error) = cache
            .set_ex::<_, _, ()>(Self::room_cache_key(room.id), data, 60 * 60)
            .await
        {
            tracing::warn!(%error, room_id = %room.id, "redis cache write skipped");
        }
        Ok(())
    }

    async fn room_by_id_from_database(&self, id: Uuid) -> Result<Option<GameRoom>, GameError> {
        let row: Option<(serde_json::Value, i64)> =
            sqlx::query_as("SELECT snapshot, persistence_revision FROM game_rooms WHERE id=$1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        row.map(|(snapshot, revision)| {
            let mut room: GameRoom =
                serde_json::from_value(snapshot).map_err(|_| GameError::Internal)?;
            room.persistence_revision = revision.max(0) as u64;
            Ok(room)
        })
        .transpose()
    }

    async fn persist_room(
        &self,
        room: &mut GameRoom,
        lease: Option<RoomAuthorityLease>,
    ) -> Result<(), GameError> {
        let expected_revision =
            i64::try_from(room.persistence_revision).map_err(|_| GameError::Internal)?;
        let next_revision = expected_revision
            .checked_add(1)
            .ok_or(GameError::Internal)?;
        let mut persisted = room.clone();
        persisted.persistence_revision = next_revision as u64;
        let snapshot = serde_json::to_value(&persisted).map_err(|_| GameError::Internal)?;
        let status = serde_json::to_value(room.status)
            .map_err(|_| GameError::Internal)?
            .as_str()
            .unwrap_or("CANCELLED")
            .to_string();
        let visibility = serde_json::to_value(room.visibility)
            .map_err(|_| GameError::Internal)?
            .as_str()
            .unwrap_or("PRIVATE")
            .to_string();
        let mut transaction = self.pool.begin().await?;
        let result = if let Some(lease) = lease {
            let fencing_token =
                i64::try_from(lease.fencing_token).map_err(|_| GameError::Internal)?;
            sqlx::query(
                "UPDATE game_rooms SET code=$2, name=$3, visibility=$4, status=$5, snapshot=$6, created_at=$7, updated_at=$8, persistence_revision=$9, authority_owner_id=NULL, authority_lease_expires_at=NULL WHERE id=$1 AND persistence_revision=$10 AND authority_owner_id=$11 AND authority_fencing_token=$12 AND authority_lease_expires_at > now()",
            )
            .bind(room.id)
            .bind(&room.code)
            .bind(&room.name)
            .bind(visibility)
            .bind(status)
            .bind(snapshot)
            .bind(room.created_at)
            .bind(room.updated_at)
            .bind(next_revision)
            .bind(expected_revision)
            .bind(lease.owner_instance_id)
            .bind(fencing_token)
            .execute(&mut *transaction)
            .await?
        } else {
            sqlx::query(
                "INSERT INTO game_rooms (id, code, name, visibility, status, snapshot, created_at, updated_at, persistence_revision) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (id) DO UPDATE SET name=$3, visibility=$4, status=$5, snapshot=$6, updated_at=$8, persistence_revision=$9 WHERE game_rooms.persistence_revision=$10 AND game_rooms.authority_owner_id IS NULL",
            )
            .bind(room.id)
            .bind(&room.code)
            .bind(&room.name)
            .bind(visibility)
            .bind(status)
            .bind(snapshot)
            .bind(room.created_at)
            .bind(room.updated_at)
            .bind(next_revision)
            .bind(expected_revision)
            .execute(&mut *transaction)
            .await?
        };
        if result.rows_affected() == 0 {
            return Err(GameError::VersionConflict);
        }
        if let Some(result) = room.game.as_ref().and_then(|game| game.result.as_ref()) {
            let participant_session_ids: Vec<_> = room
                .players
                .iter()
                .map(|player| player.session_id)
                .collect();
            let participant_identities: Vec<(Uuid, Option<Uuid>)> =
                sqlx::query_as("SELECT id, account_id FROM user_sessions WHERE id=ANY($1)")
                    .bind(&participant_session_ids)
                    .fetch_all(&mut *transaction)
                    .await?;
            let participant_account_ids: Vec<Uuid> = participant_identities
                .iter()
                .filter_map(|(_, account_id)| *account_id)
                .collect();
            let result_json = serde_json::to_value(result).map_err(|_| GameError::Internal)?;
            sqlx::query(
                "INSERT INTO game_results (room_id, room_name, participant_session_ids, participant_account_ids, result, finished_at) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (room_id) DO UPDATE SET participant_account_ids=$4, result=$5, finished_at=$6",
            )
            .bind(room.id)
            .bind(&room.name)
            .bind(participant_session_ids)
            .bind(participant_account_ids)
            .bind(result_json)
            .bind(result.finished_at)
            .execute(&mut *transaction)
            .await?;
            for player in &room.players {
                let account_id = participant_identities
                    .iter()
                    .find(|(session_id, _)| *session_id == player.session_id)
                    .and_then(|(_, account_id)| *account_id);
                sqlx::query(
                    "INSERT INTO game_result_participants (room_id, player_id, session_id, account_id) VALUES ($1,$2,$3,$4) ON CONFLICT (room_id,player_id) DO UPDATE SET session_id=$3, account_id=$4",
                )
                .bind(room.id)
                .bind(player.id)
                .bind(player.session_id)
                .bind(account_id)
                .execute(&mut *transaction)
                .await?;
            }
        }
        transaction.commit().await?;
        room.persistence_revision = persisted.persistence_revision;
        self.cache_room(&persisted).await?;
        Ok(())
    }
}

#[async_trait]
impl GameStore for PostgresRedisStore {
    async fn health_check(&self) -> Result<(), GameError> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(())
    }

    async fn save_session(&self, session: &UserSession) -> Result<(), GameError> {
        sqlx::query(
            "INSERT INTO user_sessions (id, nickname, token_hash, created_at, last_seen_at, current_room_id, account_id) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (id) DO UPDATE SET nickname=$2, token_hash=$3, last_seen_at=$5, current_room_id=$6, account_id=$7"
        )
        .bind(session.id).bind(&session.nickname).bind(&session.token_hash)
        .bind(session.created_at).bind(session.last_seen_at).bind(session.current_room_id).bind(session.account_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn session_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<UserSession>, GameError> {
        let row = sqlx::query_as::<_, (Uuid, String, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, Option<Uuid>, Option<Uuid>)>(
            "SELECT id, nickname, token_hash, created_at, last_seen_at, current_room_id, account_id FROM user_sessions WHERE token_hash=$1"
        ).bind(token_hash).fetch_optional(&self.pool).await?;
        Ok(row.map(
            |(id, nickname, token_hash, created_at, last_seen_at, current_room_id, account_id)| {
                UserSession {
                    id,
                    account_id,
                    nickname,
                    token_hash,
                    created_at,
                    last_seen_at,
                    current_room_id,
                }
            },
        ))
    }

    async fn update_session_room(
        &self,
        session_id: Uuid,
        room_id: Option<Uuid>,
    ) -> Result<(), GameError> {
        let result = sqlx::query(
            "UPDATE user_sessions SET current_room_id=$2, last_seen_at=now() WHERE id=$1",
        )
        .bind(session_id)
        .bind(room_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(GameError::Unauthorized);
        }
        Ok(())
    }

    async fn delete_session(&self, session_id: Uuid) -> Result<(), GameError> {
        sqlx::query("DELETE FROM user_sessions WHERE id=$1")
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn create_account(
        &self,
        session_id: Uuid,
        account: &PlayerAccount,
        recovery_key_hash: &str,
        next_token_hash: &str,
    ) -> Result<(), GameError> {
        let mut transaction = self.pool.begin().await?;
        let existing: Option<Option<Uuid>> =
            sqlx::query_scalar("SELECT account_id FROM user_sessions WHERE id=$1 FOR UPDATE")
                .bind(session_id)
                .fetch_optional(&mut *transaction)
                .await?;
        match existing {
            None => return Err(GameError::Unauthorized),
            Some(Some(_)) => return Err(GameError::InvalidState),
            Some(None) => {}
        }
        if let Err(error) = sqlx::query(
            "INSERT INTO player_accounts (id, handle, recovery_key_hash, created_at) VALUES ($1,$2,$3,$4)",
        )
        .bind(account.id)
        .bind(&account.handle)
        .bind(recovery_key_hash)
        .bind(account.created_at)
        .execute(&mut *transaction)
        .await
        {
            if error
                .as_database_error()
                .is_some_and(|database| database.is_unique_violation())
            {
                return Err(GameError::AccountHandleTaken);
            }
            return Err(error.into());
        }
        sqlx::query(
            "UPDATE user_sessions SET account_id=$2, nickname=$3, token_hash=$4, last_seen_at=now() WHERE id=$1",
        )
        .bind(session_id)
        .bind(account.id)
        .bind(&account.handle)
        .bind(next_token_hash)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE game_result_participants SET account_id=$2 WHERE session_id=$1 AND account_id IS NULL",
        )
        .bind(session_id)
        .bind(account.id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE game_results SET participant_account_ids=array_append(participant_account_ids,$2) WHERE $1=ANY(participant_session_ids) AND NOT ($2=ANY(participant_account_ids))",
        )
        .bind(session_id)
        .bind(account.id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn account_by_credentials(
        &self,
        account_id: Uuid,
        recovery_key_hash: &str,
    ) -> Result<Option<PlayerAccount>, GameError> {
        let row: Option<(Uuid, String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, handle, created_at FROM player_accounts WHERE id=$1 AND recovery_key_hash=$2",
        )
        .bind(account_id)
        .bind(recovery_key_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(id, handle, created_at)| PlayerAccount {
            id,
            handle,
            created_at,
        }))
    }

    async fn sessions_for_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<AccountSession>, GameError> {
        let rows: Vec<(Uuid, String, DateTime<Utc>, DateTime<Utc>, Option<Uuid>)> = sqlx::query_as(
            "SELECT id, nickname, created_at, last_seen_at, current_room_id FROM user_sessions WHERE account_id=$1 ORDER BY last_seen_at DESC",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, nickname, created_at, last_seen_at, current_room_id)| AccountSession {
                    id,
                    nickname,
                    created_at,
                    last_seen_at,
                    current_room_id,
                },
            )
            .collect())
    }

    async fn delete_account_session(
        &self,
        account_id: Uuid,
        session_id: Uuid,
    ) -> Result<bool, GameError> {
        let deleted = sqlx::query("DELETE FROM user_sessions WHERE id=$1 AND account_id=$2")
            .bind(session_id)
            .bind(account_id)
            .execute(&self.pool)
            .await?;
        Ok(deleted.rows_affected() == 1)
    }

    async fn export_account_data(
        &self,
        account_id: Uuid,
        request_id: Uuid,
        subject_fingerprint: &str,
        generated_at: DateTime<Utc>,
    ) -> Result<serde_json::Value, GameError> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .execute(&mut *transaction)
            .await?;
        let account: serde_json::Value = sqlx::query_scalar(
            "SELECT jsonb_build_object('id',id,'handle',handle,'createdAt',created_at) FROM player_accounts WHERE id=$1",
        )
        .bind(account_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(GameError::Unauthorized)?;
        let session_ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM user_sessions WHERE account_id=$1")
                .bind(account_id)
                .fetch_all(&mut *transaction)
                .await?;
        let mut identities = session_ids.clone();
        identities.push(account_id);
        let sessions: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('id',id,'nickname',nickname,'createdAt',created_at,'lastSeenAt',last_seen_at,'currentRoomId',current_room_id) FROM user_sessions WHERE account_id=$1 ORDER BY last_seen_at DESC",
        )
        .bind(account_id)
        .fetch_all(&mut *transaction)
        .await?;
        let game_history: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('roomId',results.room_id,'roomName',results.room_name,'result',results.result,'finishedAt',results.finished_at) FROM game_results results WHERE EXISTS (SELECT 1 FROM game_result_participants participant WHERE participant.room_id=results.room_id AND (participant.account_id=$1 OR participant.session_id=ANY($2))) ORDER BY results.finished_at DESC",
        )
        .bind(account_id)
        .bind(&session_ids)
        .fetch_all(&mut *transaction)
        .await?;
        let rewards: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('sourceKind',source_kind,'sourceId',source_id,'periodKey',period_key,'xp',xp,'createdAt',created_at,'reversedAt',reversed_at,'reversalReason',reversal_reason) FROM progression_reward_ledger WHERE account_id=$1 ORDER BY created_at",
        )
        .bind(account_id)
        .fetch_all(&mut *transaction)
        .await?;
        let relationships: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('targetIdentityId',target_identity_id,'targetNickname',target_nickname,'muted',muted,'blocked',blocked,'updatedAt',updated_at) FROM player_relationships WHERE actor_identity_id=ANY($1) ORDER BY updated_at DESC",
        )
        .bind(&identities)
        .fetch_all(&mut *transaction)
        .await?;
        let reports: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('id',id,'direction',CASE WHEN reporter_identity_id=ANY($1) THEN 'SUBMITTED' ELSE 'RECEIVED' END,'targetNickname',target_nickname,'category',category,'details',details,'evidence',evidence,'status',status,'createdAt',created_at,'updatedAt',updated_at) FROM player_reports WHERE reporter_identity_id=ANY($1) OR target_identity_id=ANY($1) ORDER BY created_at DESC",
        )
        .bind(&identities)
        .fetch_all(&mut *transaction)
        .await?;
        let moderation_actions: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('id',action.id,'reportId',action.report_id,'action',action.action_type,'reason',action.reason,'expiresAt',action.expires_at,'reversesActionId',action.reverses_action_id,'createdAt',action.created_at) FROM player_moderation_actions action JOIN player_reports report ON report.id=action.report_id WHERE report.reporter_identity_id=ANY($1) OR report.target_identity_id=ANY($1) ORDER BY action.created_at",
        )
        .bind(&identities)
        .fetch_all(&mut *transaction)
        .await?;
        let integrity_signals: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('id',id,'roomId',room_id,'kind',kind,'severity',severity,'confidence',confidence,'evidence',evidence,'occurrences',occurrences,'firstObservedAt',first_observed_at,'lastObservedAt',last_observed_at) FROM integrity_signals WHERE subject_identity_id=ANY($1) ORDER BY last_observed_at DESC",
        )
        .bind(&identities)
        .fetch_all(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO privacy_requests (id,subject_fingerprint,request_type,status,created_at,completed_at) VALUES ($1,$2,'EXPORT','COMPLETED',$3,$3)",
        )
        .bind(request_id)
        .bind(subject_fingerprint)
        .bind(generated_at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(serde_json::json!({
            "formatVersion": 1,
            "requestId": request_id,
            "generatedAt": generated_at,
            "account": account,
            "sessions": sessions,
            "gameHistory": game_history,
            "progressionRewards": rewards,
            "socialRelationships": relationships,
            "moderationReports": reports,
            "moderationActions": moderation_actions,
            "integritySignals": integrity_signals,
            "cacheCopies": "No independent data; Redis room cache follows the authoritative room lifecycle.",
            "credentialsExcluded": true,
        }))
    }

    async fn delete_account_data(
        &self,
        account_id: Uuid,
        request_id: Uuid,
        subject_fingerprint: &str,
        known_room_ids: &[Uuid],
        deleted_at: DateTime<Utc>,
    ) -> Result<AccountDeletionStats, GameError> {
        let mut transaction = self.pool.begin().await?;
        let account_handle: String =
            sqlx::query_scalar("SELECT handle FROM player_accounts WHERE id=$1 FOR UPDATE")
                .bind(account_id)
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or(GameError::Unauthorized)?;
        let sessions: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT id,nickname FROM user_sessions WHERE account_id=$1 FOR UPDATE")
                .bind(account_id)
                .fetch_all(&mut *transaction)
                .await?;
        let session_ids: Vec<_> = sessions.iter().map(|(id, _)| *id).collect();
        let deleted_names: Vec<_> = std::iter::once(account_handle.clone())
            .chain(sessions.iter().map(|(_, nickname)| nickname.clone()))
            .collect();
        let mut identities = session_ids.clone();
        identities.push(account_id);
        let room_rows: Vec<(Uuid, serde_json::Value)> = sqlx::query_as(
            "SELECT id,snapshot FROM game_rooms room WHERE id=ANY($2) OR EXISTS (SELECT 1 FROM jsonb_array_elements(room.snapshot->'players') player WHERE (player->>'sessionId')::uuid=ANY($1)) FOR UPDATE",
        )
        .bind(&session_ids)
        .bind(known_room_ids)
        .fetch_all(&mut *transaction)
        .await?;
        let mut replacement_session_ids = Vec::new();
        for old_session_id in &session_ids {
            replacement_session_ids.push((*old_session_id, Uuid::new_v4()));
        }
        let mut affected_room_ids = Vec::with_capacity(room_rows.len());
        for (room_id, snapshot) in room_rows {
            let mut room: GameRoom =
                serde_json::from_value(snapshot).map_err(|_| GameError::Internal)?;
            if !matches!(room.status, RoomStatus::Finished | RoomStatus::Cancelled) {
                return Err(GameError::InvalidState);
            }
            let mut deleted_player_ids = Vec::new();
            for player in &mut room.players {
                if let Some((_, replacement)) = replacement_session_ids
                    .iter()
                    .find(|(session_id, _)| *session_id == player.session_id)
                {
                    deleted_player_ids.push(player.id);
                    player.session_id = *replacement;
                    player.nickname = "Deleted Commander".to_string();
                }
            }
            for message in &mut room.chat_messages {
                for name in &deleted_names {
                    message.content = message.content.replace(name, "Deleted Commander");
                }
                if message
                    .player_id
                    .is_some_and(|player_id| deleted_player_ids.contains(&player_id))
                {
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
            let anonymized = serde_json::to_value(&room).map_err(|_| GameError::Internal)?;
            sqlx::query("UPDATE game_rooms SET name=$2,snapshot=$3,updated_at=$4,persistence_revision=persistence_revision+1 WHERE id=$1")
                .bind(room_id)
                .bind(&room.name)
                .bind(anonymized)
                .bind(deleted_at)
                .execute(&mut *transaction)
                .await?;
            sqlx::query("UPDATE game_results SET room_name='Archived Operation' WHERE room_id=$1")
                .bind(room_id)
                .execute(&mut *transaction)
                .await?;
            affected_room_ids.push(room_id);
        }
        for (old_session_id, replacement_session_id) in &replacement_session_ids {
            sqlx::query("UPDATE game_result_participants SET session_id=$2,account_id=NULL WHERE session_id=$1")
                .bind(old_session_id)
                .bind(replacement_session_id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query("UPDATE game_results SET participant_session_ids=array_replace(participant_session_ids,$1,$2),participant_account_ids=array_remove(participant_account_ids,$3) WHERE $1=ANY(participant_session_ids) OR $3=ANY(participant_account_ids)")
                .bind(old_session_id)
                .bind(replacement_session_id)
                .bind(account_id)
                .execute(&mut *transaction)
                .await?;
        }
        let rewards_deleted =
            sqlx::query("DELETE FROM progression_reward_ledger WHERE account_id=$1")
                .bind(account_id)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
        let relationships_deleted = sqlx::query(
            "DELETE FROM player_relationships WHERE actor_identity_id=ANY($1) OR target_identity_id=ANY($1)",
        )
        .bind(&identities)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        let reports_deleted = sqlx::query(
            "DELETE FROM player_reports WHERE reporter_identity_id=ANY($1) OR target_identity_id=ANY($1)",
        )
        .bind(&identities)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        let integrity_signals_deleted =
            sqlx::query("DELETE FROM integrity_signals WHERE subject_identity_id=ANY($1)")
                .bind(&identities)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
        sqlx::query("DELETE FROM matchmaking_queue WHERE session_id=ANY($1)")
            .bind(&session_ids)
            .execute(&mut *transaction)
            .await?;
        let sessions_deleted = sqlx::query("DELETE FROM user_sessions WHERE account_id=$1")
            .bind(account_id)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
        sqlx::query("DELETE FROM player_accounts WHERE id=$1")
            .bind(account_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO privacy_requests (id,subject_fingerprint,request_type,status,created_at,completed_at) VALUES ($1,$2,'DELETE','COMPLETED',$3,$3)",
        )
        .bind(request_id)
        .bind(subject_fingerprint)
        .bind(deleted_at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        if let Some(mut cache) = self.cache.clone() {
            for room_id in &affected_room_ids {
                if let Err(error) = cache.del::<_, ()>(Self::room_cache_key(*room_id)).await {
                    tracing::warn!(%error, %room_id, "account deletion cache eviction failed");
                }
            }
        }
        Ok(AccountDeletionStats {
            sessions_deleted,
            rewards_deleted,
            relationships_deleted,
            reports_deleted,
            integrity_signals_deleted,
            rooms_anonymized: affected_room_ids.len() as u64,
        })
    }

    async fn save_room(&self, room: &mut GameRoom) -> Result<(), GameError> {
        self.persist_room(room, None).await
    }

    async fn acquire_room_authority(
        &self,
        room_id: Uuid,
        owner_instance_id: Uuid,
        lease_duration: Duration,
    ) -> Result<Option<RoomAuthorityLease>, GameError> {
        let lease_millis = i64::try_from(lease_duration.as_millis().max(1)).unwrap_or(i64::MAX);
        let token: Option<i64> = sqlx::query_scalar(
            "UPDATE game_rooms SET authority_owner_id=$2, authority_fencing_token=authority_fencing_token+1, authority_lease_expires_at=now()+($3 * interval '1 millisecond') WHERE id=$1 AND (authority_owner_id IS NULL OR authority_owner_id=$2 OR authority_lease_expires_at <= now()) RETURNING authority_fencing_token",
        )
        .bind(room_id)
        .bind(owner_instance_id)
        .bind(lease_millis)
        .fetch_optional(&self.pool)
        .await?;
        token
            .map(|fencing_token| {
                Ok(RoomAuthorityLease {
                    room_id,
                    owner_instance_id,
                    fencing_token: u64::try_from(fencing_token).map_err(|_| GameError::Internal)?,
                })
            })
            .transpose()
    }

    async fn save_room_fenced(
        &self,
        room: &mut GameRoom,
        lease: RoomAuthorityLease,
    ) -> Result<(), GameError> {
        if room.id != lease.room_id {
            return Err(GameError::VersionConflict);
        }
        self.persist_room(room, Some(lease)).await
    }

    async fn release_room_authority(&self, lease: RoomAuthorityLease) -> Result<(), GameError> {
        let fencing_token = i64::try_from(lease.fencing_token).map_err(|_| GameError::Internal)?;
        sqlx::query(
            "UPDATE game_rooms SET authority_owner_id=NULL, authority_lease_expires_at=NULL WHERE id=$1 AND authority_owner_id=$2 AND authority_fencing_token=$3",
        )
        .bind(lease.room_id)
        .bind(lease.owner_instance_id)
        .bind(fencing_token)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn room_by_id(&self, id: Uuid) -> Result<Option<GameRoom>, GameError> {
        // A stale cached snapshot is unacceptable for an authoritative game mutation. PostgreSQL
        // remains the read authority until the distributed room-owner protocol can provide fenced,
        // revision-aware cache reads.
        let room = self.room_by_id_from_database(id).await?;
        if let Some(room) = &room {
            self.cache_room(room).await?;
        }
        Ok(room)
    }

    async fn room_by_id_authoritative(&self, id: Uuid) -> Result<Option<GameRoom>, GameError> {
        let room = self.room_by_id_from_database(id).await?;
        if let Some(room) = &room {
            self.cache_room(room).await?;
        }
        Ok(room)
    }

    async fn room_by_code(&self, code: &str) -> Result<Option<GameRoom>, GameError> {
        let row: Option<(serde_json::Value, i64)> =
            sqlx::query_as("SELECT snapshot, persistence_revision FROM game_rooms WHERE code=$1")
                .bind(code)
                .fetch_optional(&self.pool)
                .await?;
        let room = row
            .map(|(snapshot, revision)| {
                let mut room: GameRoom =
                    serde_json::from_value(snapshot).map_err(|_| GameError::Internal)?;
                room.persistence_revision = revision.max(0) as u64;
                Ok::<GameRoom, GameError>(room)
            })
            .transpose()?;
        if let Some(room) = &room {
            self.cache_room(room).await?;
        }
        Ok(room)
    }

    async fn active_rooms(&self) -> Result<Vec<GameRoom>, GameError> {
        let snapshots: Vec<(serde_json::Value, i64)> = sqlx::query_as(
            "SELECT snapshot, persistence_revision FROM game_rooms WHERE status NOT IN ('FINISHED', 'CANCELLED')",
        )
        .fetch_all(&self.pool)
        .await?;
        snapshots
            .into_iter()
            .map(|(value, revision)| {
                let mut room: GameRoom =
                    serde_json::from_value(value).map_err(|_| GameError::Internal)?;
                room.persistence_revision = revision.max(0) as u64;
                Ok(room)
            })
            .collect()
    }

    async fn list_public_rooms(&self) -> Result<Vec<RoomSummary>, GameError> {
        let snapshots: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT snapshot FROM game_rooms WHERE visibility='PUBLIC' AND status='WAITING_FOR_OPPONENT' ORDER BY created_at DESC LIMIT 100"
        ).fetch_all(&self.pool).await?;
        snapshots
            .into_iter()
            .map(|value| {
                serde_json::from_value::<GameRoom>(value)
                    .map(|room| room.summary())
                    .map_err(|_| GameError::Internal)
            })
            .collect()
    }

    async fn history_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Vec<GameHistoryItem>, GameError> {
        let rows: Vec<(Uuid, String, serde_json::Value, Uuid)> = sqlx::query_as(
            "WITH identity AS (SELECT account_id FROM user_sessions WHERE id=$1) SELECT results.room_id, results.room_name, results.result, participants.player_id FROM game_results results JOIN game_result_participants participants ON participants.room_id=results.room_id WHERE participants.session_id=$1 OR ((SELECT account_id FROM identity) IS NOT NULL AND participants.account_id=(SELECT account_id FROM identity)) ORDER BY results.finished_at DESC LIMIT 5000"
        ).bind(session_id).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|(room_id, room_name, value, self_player_id)| {
                let result = serde_json::from_value(value).map_err(|_| GameError::Internal)?;
                Ok(GameHistoryItem {
                    room_id,
                    room_name,
                    self_player_id,
                    result,
                })
            })
            .collect()
    }

    async fn enqueue_matchmaking(
        &self,
        session: &UserSession,
    ) -> Result<MatchmakingEnqueueResult, GameError> {
        let mut transaction = self.pool.begin().await?;
        let current_room_id: Option<Option<Uuid>> =
            sqlx::query_scalar("SELECT current_room_id FROM user_sessions WHERE id=$1 FOR UPDATE")
                .bind(session.id)
                .fetch_optional(&mut *transaction)
                .await?;
        match current_room_id {
            None => return Err(GameError::Unauthorized),
            Some(Some(_)) => return Err(GameError::AlreadyJoined),
            Some(None) => {}
        }

        sqlx::query(
            "UPDATE matchmaking_queue SET claim_id=NULL, claimed_at=NULL WHERE claimed_at < now() - interval '30 seconds'",
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "DELETE FROM matchmaking_queue WHERE claim_id IS NULL AND queued_at < now() - interval '10 minutes'",
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO matchmaking_queue (session_id, queued_at) VALUES ($1, now()) ON CONFLICT (session_id) DO NOTHING",
        )
        .bind(session.id)
        .execute(&mut *transaction)
        .await?;

        let (queued_at, existing_claim): (DateTime<Utc>, Option<Uuid>) = sqlx::query_as(
            "SELECT queued_at, claim_id FROM matchmaking_queue WHERE session_id=$1 FOR UPDATE",
        )
        .bind(session.id)
        .fetch_one(&mut *transaction)
        .await?;
        if existing_claim.is_some() {
            transaction.commit().await?;
            return Ok(MatchmakingEnqueueResult {
                queued_at,
                claim: None,
            });
        }

        let own_identity = session.account_id.unwrap_or(session.id);
        let opponent: Option<(
            Uuid,
            String,
            String,
            DateTime<Utc>,
            DateTime<Utc>,
            Option<Uuid>,
            Option<Uuid>,
        )> = sqlx::query_as(
            "SELECT sessions.id, sessions.nickname, sessions.token_hash, sessions.created_at, sessions.last_seen_at, sessions.current_room_id, sessions.account_id FROM matchmaking_queue queue JOIN user_sessions sessions ON sessions.id=queue.session_id WHERE queue.session_id<>$1 AND queue.claim_id IS NULL AND sessions.current_room_id IS NULL AND NOT EXISTS (SELECT 1 FROM player_relationships relationships WHERE relationships.blocked AND ((relationships.actor_identity_id=$2 AND relationships.target_identity_id=COALESCE(sessions.account_id,sessions.id)) OR (relationships.actor_identity_id=COALESCE(sessions.account_id,sessions.id) AND relationships.target_identity_id=$2))) ORDER BY queue.queued_at ASC FOR UPDATE OF queue SKIP LOCKED LIMIT 1",
        )
        .bind(session.id)
        .bind(own_identity)
        .fetch_optional(&mut *transaction)
        .await?;

        let Some((id, nickname, token_hash, created_at, last_seen_at, current_room_id, account_id)) =
            opponent
        else {
            transaction.commit().await?;
            return Ok(MatchmakingEnqueueResult {
                queued_at,
                claim: None,
            });
        };
        let claim_id = Uuid::new_v4();
        let claimed = sqlx::query(
            "UPDATE matchmaking_queue SET claim_id=$1, claimed_at=now() WHERE session_id=ANY($2) AND claim_id IS NULL",
        )
        .bind(claim_id)
        .bind(vec![session.id, id])
        .execute(&mut *transaction)
        .await?;
        if claimed.rows_affected() != 2 {
            return Err(GameError::VersionConflict);
        }
        transaction.commit().await?;

        Ok(MatchmakingEnqueueResult {
            queued_at,
            claim: Some(MatchmakingClaim {
                id: claim_id,
                opponent: UserSession {
                    id,
                    account_id,
                    nickname,
                    token_hash,
                    created_at,
                    last_seen_at,
                    current_room_id,
                },
            }),
        })
    }

    async fn complete_matchmaking(
        &self,
        claim_id: Uuid,
        room: &mut GameRoom,
    ) -> Result<(), GameError> {
        let mut transaction = self.pool.begin().await?;
        let mut claimed_session_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT session_id FROM matchmaking_queue WHERE claim_id=$1 ORDER BY session_id FOR UPDATE",
        )
        .bind(claim_id)
        .fetch_all(&mut *transaction)
        .await?;
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
        if room.persistence_revision != 0 {
            return Err(GameError::VersionConflict);
        }

        let mut persisted = room.clone();
        persisted.persistence_revision = 1;
        let snapshot = serde_json::to_value(&persisted).map_err(|_| GameError::Internal)?;
        let status = serde_json::to_value(room.status)
            .map_err(|_| GameError::Internal)?
            .as_str()
            .unwrap_or("CANCELLED")
            .to_string();
        let visibility = serde_json::to_value(room.visibility)
            .map_err(|_| GameError::Internal)?
            .as_str()
            .unwrap_or("PRIVATE")
            .to_string();
        let inserted = sqlx::query(
            "INSERT INTO game_rooms (id, code, name, visibility, status, snapshot, created_at, updated_at, persistence_revision) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,1) ON CONFLICT (id) DO NOTHING",
        )
        .bind(room.id)
        .bind(&room.code)
        .bind(&room.name)
        .bind(visibility)
        .bind(status)
        .bind(snapshot)
        .bind(room.created_at)
        .bind(room.updated_at)
        .execute(&mut *transaction)
        .await?;
        if inserted.rows_affected() != 1 {
            return Err(GameError::VersionConflict);
        }

        let sessions_updated = sqlx::query(
            "UPDATE user_sessions SET current_room_id=$1, last_seen_at=now() WHERE id=ANY($2) AND current_room_id IS NULL",
        )
        .bind(room.id)
        .bind(&claimed_session_ids)
        .execute(&mut *transaction)
        .await?;
        if sessions_updated.rows_affected() != 2 {
            return Err(GameError::AlreadyJoined);
        }
        let removed = sqlx::query("DELETE FROM matchmaking_queue WHERE claim_id=$1")
            .bind(claim_id)
            .execute(&mut *transaction)
            .await?;
        if removed.rows_affected() != 2 {
            return Err(GameError::VersionConflict);
        }
        transaction.commit().await?;
        room.persistence_revision = persisted.persistence_revision;
        self.cache_room(&persisted).await?;
        Ok(())
    }

    async fn release_matchmaking_claim(&self, claim_id: Uuid) -> Result<(), GameError> {
        sqlx::query(
            "UPDATE matchmaking_queue SET claim_id=NULL, claimed_at=NULL WHERE claim_id=$1",
        )
        .bind(claim_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn cancel_matchmaking(&self, session_id: Uuid) -> Result<bool, GameError> {
        let result =
            sqlx::query("DELETE FROM matchmaking_queue WHERE session_id=$1 AND claim_id IS NULL")
                .bind(session_id)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn mission_rewards(&self, account_id: Uuid) -> Result<Vec<MissionReward>, GameError> {
        let rows: Vec<(String, String, i32)> = sqlx::query_as(
            "SELECT source_id, period_key, xp FROM progression_reward_ledger WHERE account_id=$1 AND source_kind='MISSION' AND reversed_at IS NULL ORDER BY created_at",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(mission_id, period_key, xp)| {
                Ok(MissionReward {
                    mission_id,
                    period_key,
                    xp: u32::try_from(xp).map_err(|_| GameError::Internal)?,
                })
            })
            .collect()
    }

    async fn claim_mission_reward(
        &self,
        account_id: Uuid,
        mission_id: &str,
        period_key: &str,
        xp: u32,
    ) -> Result<bool, GameError> {
        let result = sqlx::query(
            "INSERT INTO progression_reward_ledger (id,account_id,source_kind,source_id,period_key,xp) VALUES ($1,$2,'MISSION',$3,$4,$5) ON CONFLICT (account_id,source_kind,source_id,period_key) DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(account_id)
        .bind(mission_id)
        .bind(period_key)
        .bind(i32::try_from(xp).map_err(|_| GameError::Internal)?)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn identity_for_session(&self, session_id: Uuid) -> Result<Option<Uuid>, GameError> {
        sqlx::query_scalar("SELECT COALESCE(account_id,id) FROM user_sessions WHERE id=$1")
            .bind(session_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Into::into)
    }

    async fn set_social_relationship(
        &self,
        actor_identity_id: Uuid,
        relationship: SocialRelationship,
    ) -> Result<(), GameError> {
        if !relationship.muted && !relationship.blocked {
            sqlx::query(
                "DELETE FROM player_relationships WHERE actor_identity_id=$1 AND target_identity_id=$2",
            )
            .bind(actor_identity_id)
            .bind(relationship.target_identity_id)
            .execute(&self.pool)
            .await?;
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO player_relationships (actor_identity_id,target_identity_id,target_nickname,muted,blocked,updated_at) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (actor_identity_id,target_identity_id) DO UPDATE SET target_nickname=$3,muted=$4,blocked=$5,updated_at=$6",
        )
        .bind(actor_identity_id)
        .bind(relationship.target_identity_id)
        .bind(&relationship.target_nickname)
        .bind(relationship.muted)
        .bind(relationship.blocked)
        .bind(relationship.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn social_relationships(
        &self,
        actor_identity_id: Uuid,
    ) -> Result<Vec<SocialRelationship>, GameError> {
        let rows: Vec<(Uuid, String, bool, bool, DateTime<Utc>)> = sqlx::query_as(
            "SELECT target_identity_id,target_nickname,muted,blocked,updated_at FROM player_relationships WHERE actor_identity_id=$1 ORDER BY updated_at DESC",
        )
        .bind(actor_identity_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(target_identity_id, target_nickname, muted, blocked, updated_at)| {
                    SocialRelationship {
                        target_identity_id,
                        target_nickname,
                        muted,
                        blocked,
                        updated_at,
                    }
                },
            )
            .collect())
    }

    async fn social_relationship_between(
        &self,
        actor_identity_id: Uuid,
        target_identity_id: Uuid,
    ) -> Result<Option<SocialRelationship>, GameError> {
        let row: Option<(String, bool, bool, DateTime<Utc>)> = sqlx::query_as(
            "SELECT target_nickname,muted,blocked,updated_at FROM player_relationships WHERE actor_identity_id=$1 AND target_identity_id=$2",
        )
        .bind(actor_identity_id)
        .bind(target_identity_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(
            |(target_nickname, muted, blocked, updated_at)| SocialRelationship {
                target_identity_id,
                target_nickname,
                muted,
                blocked,
                updated_at,
            },
        ))
    }

    async fn create_player_report(&self, report: &NewPlayerReport) -> Result<(), GameError> {
        sqlx::query(
            "INSERT INTO player_reports (id,reporter_identity_id,target_identity_id,room_id,target_player_id,target_nickname,category,details,evidence,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10)",
        )
        .bind(report.id)
        .bind(report.reporter_identity_id)
        .bind(report.target_identity_id)
        .bind(report.room_id)
        .bind(report.target_player_id)
        .bind(&report.target_nickname)
        .bind(report.category.as_str())
        .bind(&report.details)
        .bind(&report.evidence)
        .bind(report.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn moderation_cases(
        &self,
        search: Option<&str>,
        status: Option<ReportStatus>,
        before: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<ModerationCasePage, GameError> {
        type ReportRow = (
            Uuid,
            Uuid,
            Uuid,
            Uuid,
            Uuid,
            String,
            String,
            String,
            serde_json::Value,
            String,
            DateTime<Utc>,
            DateTime<Utc>,
        );
        let search_pattern = search.map(|value| format!("%{}%", value.to_lowercase()));
        let fetch_limit =
            i64::try_from(limit.saturating_add(1)).map_err(|_| GameError::Internal)?;
        let rows: Vec<ReportRow> = sqlx::query_as(
            "SELECT id,reporter_identity_id,target_identity_id,room_id,target_player_id,target_nickname,category,details,evidence,status,created_at,updated_at FROM player_reports WHERE ($1::text IS NULL OR status=$1) AND ($2::timestamptz IS NULL OR created_at < $2) AND ($3::text IS NULL OR lower(target_nickname) LIKE $3 OR lower(details) LIKE $3 OR lower(evidence::text) LIKE $3) ORDER BY created_at DESC,id DESC LIMIT $4",
        )
        .bind(status.map(ReportStatus::as_str))
        .bind(before)
        .bind(search_pattern)
        .bind(fetch_limit)
        .fetch_all(&self.pool)
        .await?;
        let has_more = rows.len() > limit;
        let mut cases = Vec::with_capacity(rows.len().min(limit));
        for row in rows.into_iter().take(limit) {
            let report = PlayerReport {
                id: row.0,
                reporter_identity_id: row.1,
                target_identity_id: row.2,
                room_id: row.3,
                target_player_id: row.4,
                target_nickname: row.5,
                category: ReportCategory::parse(&row.6).ok_or(GameError::Internal)?,
                details: row.7,
                evidence: row.8,
                status: ReportStatus::parse(&row.9).ok_or(GameError::Internal)?,
                created_at: row.10,
                updated_at: row.11,
            };
            let action_rows: Vec<(
                Uuid,
                Uuid,
                String,
                String,
                String,
                Option<DateTime<Utc>>,
                Option<Uuid>,
                DateTime<Utc>,
            )> = sqlx::query_as(
                "SELECT id,target_identity_id,operator_id,action_type,reason,expires_at,reverses_action_id,created_at FROM player_moderation_actions WHERE report_id=$1 ORDER BY created_at,id",
            )
            .bind(report.id)
            .fetch_all(&self.pool)
            .await?;
            let actions = action_rows
                .into_iter()
                .map(|action| {
                    Ok(ModerationAction {
                        id: action.0,
                        report_id: report.id,
                        target_identity_id: action.1,
                        operator_id: action.2,
                        action: ModerationActionKind::parse(&action.3)
                            .ok_or(GameError::Internal)?,
                        reason: action.4,
                        expires_at: action.5,
                        reverses_action_id: action.6,
                        created_at: action.7,
                    })
                })
                .collect::<Result<Vec<_>, GameError>>()?;
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
        let mut transaction = self.pool.begin().await?;
        let target_identity_id: Uuid = sqlx::query_scalar(
            "SELECT target_identity_id FROM player_reports WHERE id=$1 FOR UPDATE",
        )
        .bind(action.report_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(GameError::ReportNotFound)?;
        if action.action == ModerationActionKind::Reverse {
            let reversed_id = action.reverses_action_id.ok_or(GameError::InvalidRequest)?;
            let reversed: Option<(Uuid, Uuid, String)> = sqlx::query_as(
                "SELECT report_id,target_identity_id,action_type FROM player_moderation_actions WHERE id=$1 FOR UPDATE",
            )
            .bind(reversed_id)
            .fetch_optional(&mut *transaction)
            .await?;
            let Some((report_id, target_id, action_type)) = reversed else {
                return Err(GameError::InvalidRequest);
            };
            let already_reversed: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM player_moderation_actions WHERE reverses_action_id=$1)",
            )
            .bind(reversed_id)
            .fetch_one(&mut *transaction)
            .await?;
            if report_id != action.report_id
                || target_id != target_identity_id
                || matches!(action_type.as_str(), "REVERSE" | "DISMISS")
                || already_reversed
            {
                return Err(GameError::InvalidRequest);
            }
        } else if action.reverses_action_id.is_some() {
            return Err(GameError::InvalidRequest);
        }
        sqlx::query(
            "INSERT INTO player_moderation_actions (id,report_id,target_identity_id,operator_id,action_type,reason,expires_at,reverses_action_id,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(action.id)
        .bind(action.report_id)
        .bind(target_identity_id)
        .bind(&action.operator_id)
        .bind(action.action.as_str())
        .bind(&action.reason)
        .bind(action.expires_at)
        .bind(action.reverses_action_id)
        .bind(action.created_at)
        .execute(&mut *transaction)
        .await?;
        let status = match action.action {
            ModerationActionKind::Dismiss => ReportStatus::Dismissed,
            ModerationActionKind::Reverse => ReportStatus::Reviewing,
            _ => ReportStatus::Actioned,
        };
        sqlx::query("UPDATE player_reports SET status=$2,updated_at=$3 WHERE id=$1")
            .bind(action.report_id)
            .bind(status.as_str())
            .bind(action.created_at)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(ModerationAction {
            id: action.id,
            report_id: action.report_id,
            target_identity_id,
            operator_id: action.operator_id.clone(),
            action: action.action,
            reason: action.reason.clone(),
            expires_at: action.expires_at,
            reverses_action_id: action.reverses_action_id,
            created_at: action.created_at,
        })
    }

    async fn active_penalty(
        &self,
        identity_id: Uuid,
        session_id: Uuid,
    ) -> Result<Option<ActivePenalty>, GameError> {
        let row: Option<(String, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT action_type,expires_at FROM player_moderation_actions action WHERE (target_identity_id=$1 OR target_identity_id=$2 OR target_identity_id IN (SELECT id FROM user_sessions WHERE account_id=$1)) AND action_type IN ('BAN','SUSPEND') AND (action_type='BAN' OR expires_at > now()) AND NOT EXISTS (SELECT 1 FROM player_moderation_actions reversal WHERE reversal.reverses_action_id=action.id) ORDER BY (action_type='BAN') DESC,expires_at DESC NULLS FIRST LIMIT 1",
        )
        .bind(identity_id)
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some((action, _)) if action == "BAN" => Ok(Some(ActivePenalty::Banned)),
            Some((action, Some(expires_at))) if action == "SUSPEND" => {
                Ok(Some(ActivePenalty::Suspended(expires_at)))
            }
            Some(_) => Err(GameError::Internal),
            None => Ok(None),
        }
    }

    async fn session_ids_for_identity(&self, identity_id: Uuid) -> Result<Vec<Uuid>, GameError> {
        sqlx::query_scalar("SELECT id FROM user_sessions WHERE id=$1 OR account_id=$1")
            .bind(identity_id)
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
    }

    async fn record_integrity_signal(
        &self,
        signal: &NewIntegritySignal,
    ) -> Result<IntegritySignal, GameError> {
        type SignalRow = (
            Uuid,
            Uuid,
            Option<Uuid>,
            String,
            i16,
            f64,
            serde_json::Value,
            i32,
            DateTime<Utc>,
            DateTime<Utc>,
        );
        let row: SignalRow = sqlx::query_as(
            "INSERT INTO integrity_signals (id,subject_identity_id,room_id,kind,severity,confidence,evidence,first_observed_at,last_observed_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8) ON CONFLICT (subject_identity_id,room_id,kind) WHERE room_id IS NOT NULL DO UPDATE SET severity=GREATEST(integrity_signals.severity,EXCLUDED.severity),confidence=GREATEST(integrity_signals.confidence,EXCLUDED.confidence),evidence=EXCLUDED.evidence,occurrences=integrity_signals.occurrences+1,last_observed_at=EXCLUDED.last_observed_at RETURNING id,subject_identity_id,room_id,kind,severity,confidence,evidence,occurrences,first_observed_at,last_observed_at",
        )
        .bind(signal.id)
        .bind(signal.subject_identity_id)
        .bind(signal.room_id)
        .bind(signal.kind.as_str())
        .bind(i16::from(signal.severity))
        .bind(signal.confidence)
        .bind(&signal.evidence)
        .bind(signal.observed_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(IntegritySignal {
            id: row.0,
            subject_identity_id: row.1,
            room_id: row.2,
            kind: IntegritySignalKind::parse(&row.3).ok_or(GameError::Internal)?,
            severity: u8::try_from(row.4).map_err(|_| GameError::Internal)?,
            confidence: row.5,
            evidence: row.6,
            occurrences: u32::try_from(row.7).map_err(|_| GameError::Internal)?,
            first_observed_at: row.8,
            last_observed_at: row.9,
        })
    }

    async fn integrity_signals(
        &self,
        search: Option<&str>,
        kind: Option<IntegritySignalKind>,
        before: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<IntegritySignalPage, GameError> {
        let search_pattern = search.map(|value| format!("%{}%", value.to_lowercase()));
        let fetch_limit =
            i64::try_from(limit.saturating_add(1)).map_err(|_| GameError::Internal)?;
        type SignalRow = (
            Uuid,
            Uuid,
            Option<Uuid>,
            String,
            i16,
            f64,
            serde_json::Value,
            i32,
            DateTime<Utc>,
            DateTime<Utc>,
        );
        let rows: Vec<SignalRow> = sqlx::query_as(
            "SELECT id,subject_identity_id,room_id,kind,severity,confidence,evidence,occurrences,first_observed_at,last_observed_at FROM integrity_signals WHERE ($1::text IS NULL OR kind=$1) AND ($2::timestamptz IS NULL OR last_observed_at < $2) AND ($3::text IS NULL OR lower(subject_identity_id::text) LIKE $3 OR lower(evidence::text) LIKE $3) ORDER BY severity DESC,last_observed_at DESC,id DESC LIMIT $4",
        )
        .bind(kind.map(IntegritySignalKind::as_str))
        .bind(before)
        .bind(search_pattern)
        .bind(fetch_limit)
        .fetch_all(&self.pool)
        .await?;
        let has_more = rows.len() > limit;
        let signals = rows
            .into_iter()
            .take(limit)
            .map(|row| {
                Ok(IntegritySignal {
                    id: row.0,
                    subject_identity_id: row.1,
                    room_id: row.2,
                    kind: IntegritySignalKind::parse(&row.3).ok_or(GameError::Internal)?,
                    severity: u8::try_from(row.4).map_err(|_| GameError::Internal)?,
                    confidence: row.5,
                    evidence: row.6,
                    occurrences: u32::try_from(row.7).map_err(|_| GameError::Internal)?,
                    first_observed_at: row.8,
                    last_observed_at: row.9,
                })
            })
            .collect::<Result<Vec<_>, GameError>>()?;
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
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT results.room_id) FROM game_results results JOIN game_result_participants first_player ON first_player.room_id=results.room_id JOIN game_result_participants second_player ON second_player.room_id=results.room_id AND second_player.player_id<>first_player.player_id WHERE ((COALESCE(first_player.account_id,first_player.session_id)=$1) OR first_player.session_id IN (SELECT id FROM user_sessions WHERE account_id=$1)) AND ((COALESCE(second_player.account_id,second_player.session_id)=$2) OR second_player.session_id IN (SELECT id FROM user_sessions WHERE account_id=$2)) AND results.finished_at >= $3 AND COALESCE((results.result->>'totalTurns')::integer,999) <= 5 AND results.result->>'finishReason' IN ('SURRENDER','DISCONNECT_TIMEOUT','PLAYER_LEFT')",
        )
        .bind(first_identity_id)
        .bind(second_identity_id)
        .bind(since)
        .fetch_one(&self.pool)
        .await?;
        u64::try_from(count).map_err(|_| GameError::Internal)
    }

    async fn matchmaking_time(&self, session_id: Uuid) -> Result<Option<DateTime<Utc>>, GameError> {
        sqlx::query_scalar("SELECT queued_at FROM matchmaking_queue WHERE session_id=$1")
            .bind(session_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Into::into)
    }

    async fn matchmaking_queue_stats(&self) -> Result<MatchmakingQueueStats, GameError> {
        let (queued, oldest_age_seconds): (i64, i64) = sqlx::query_as(
            "SELECT count(*)::bigint, COALESCE(EXTRACT(EPOCH FROM now()-min(queued_at)), 0)::bigint FROM matchmaking_queue",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(MatchmakingQueueStats {
            queued: queued.max(0) as u64,
            oldest_age_seconds: oldest_age_seconds.max(0) as u64,
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
        let mut transaction = self.pool.begin().await?;
        let matchmaking_entries_deleted =
            sqlx::query("DELETE FROM matchmaking_queue WHERE queued_at < $1")
                .bind(abandoned_matchmaking_before)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
        let expired_room_ids: Vec<Uuid> = sqlx::query_scalar(
            "DELETE FROM game_rooms WHERE status IN ('FINISHED','CANCELLED') AND updated_at < $1 RETURNING id",
        )
        .bind(completed_room_before)
        .fetch_all(&mut *transaction)
        .await?;
        let sessions_deleted = sqlx::query(
            "DELETE FROM user_sessions WHERE current_room_id IS NULL AND last_seen_at < $1",
        )
        .bind(inactive_session_before)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        let moderation_cases_deleted = sqlx::query(
            "DELETE FROM player_reports WHERE status IN ('ACTIONED','DISMISSED') AND updated_at < $1",
        )
        .bind(closed_moderation_before)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        let integrity_signals_deleted =
            sqlx::query("DELETE FROM integrity_signals WHERE last_observed_at < $1")
                .bind(integrity_signal_before)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
        transaction.commit().await?;

        if let Some(mut cache) = self.cache.clone() {
            for room_id in &expired_room_ids {
                if let Err(error) = cache.del::<_, ()>(Self::room_cache_key(*room_id)).await {
                    tracing::warn!(%error, %room_id, "expired room cache eviction failed");
                }
            }
        }
        Ok(RetentionStats {
            sessions_deleted,
            rooms_deleted: expired_room_ids.len() as u64,
            matchmaking_entries_deleted,
            moderation_cases_deleted,
            integrity_signals_deleted,
        })
    }

    fn kind(&self) -> &'static str {
        "postgres+redis"
    }
}
