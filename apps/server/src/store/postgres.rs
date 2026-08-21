use std::{collections::HashSet, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use redis::{AsyncCommands, aio::ConnectionManager};
use sqlx::{PgPool, Postgres, Transaction, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::{
    domain::{
        AccountSession, ActivePenalty, BalanceManifest, BalancePin, ChatMessageType, GameRoom,
        IntegritySignal, IntegritySignalKind, IntegritySignalPage, LiveContentRevision,
        MatchmakingCriteria, MatchmakingPool, MatchmakingRegion, ModerationAction,
        ModerationActionKind, ModerationCase, ModerationCasePage, NewIntegritySignal,
        NewModerationAction, NewPlayerReport, NewSupportAction, PlayerAccount, PlayerReport,
        RANKED_LEADERBOARD_MAX_LIMIT, RECENT_OPPONENT_LOOKBACK_MINUTES, RankedLeaderboardEntry,
        RankedLeaderboardPage, RankedLeaderboardSeason, RankedProfile, RankedStandingRecord,
        RankedTier, ReportCategory, ReportStatus, RoomStatus, RoomSummary, SafetyRelationship,
        SupportAccountSnapshot, SupportAction, SupportActionKind, UserSession, matchmaking_quality,
        next_season_seed, ranked_match_reward_xp, ranked_placement_reward_xp, ranked_season_key,
    },
    error::GameError,
};

use super::{
    AccountDeletionScope, AccountDeletionStats, GameHistoryItem, GameStore, MatchmakingClaim,
    MatchmakingEnqueueResult, MatchmakingQueueEntry, MatchmakingQueueStats, MissionReward,
    RankedRating, RetentionStats, RoomAuthorityLease,
};

const DELETION_RESURRECTION_COUNT_QUERY: &str = "SELECT (SELECT count(*) FROM player_accounts account JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=account.id) + (SELECT count(*) FROM user_sessions session JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=session.account_id) + (SELECT count(*) FROM progression_reward_ledger reward JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=reward.account_id) + (SELECT count(*) FROM ranked_ratings rating JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=rating.account_id) + (SELECT count(*) FROM ranked_reward_ledger reward JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=reward.account_id) + (SELECT count(*) FROM ranked_season_standings standing JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=standing.account_id) + (SELECT count(*) FROM ranked_match_participants participant JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=participant.account_id) + (SELECT count(*) FROM ranked_leaderboard_snapshot_entries entry JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=entry.account_id) + (SELECT count(*) FROM game_result_participants participant JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=participant.account_id) + (SELECT count(*) FROM game_results result JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=ANY(result.participant_account_ids)) + (SELECT count(*) FROM player_relationships relationship JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=relationship.actor_identity_id OR tombstone.account_id=relationship.target_identity_id) + (SELECT count(*) FROM player_reports report JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=report.reporter_identity_id OR tombstone.account_id=report.target_identity_id) + (SELECT count(*) FROM player_moderation_actions action JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=action.target_identity_id) + (SELECT count(*) FROM integrity_signals signal JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=signal.subject_identity_id) + (SELECT count(*) FROM player_support_actions action JOIN privacy_deletion_tombstones tombstone ON tombstone.account_id=action.account_id)";
const LIVE_CONTENT_ADVISORY_LOCK: i64 = 7_190_120_260;
const REDIS_INITIAL_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

async fn run_compatible_migrations(
    pool: &PgPool,
    operation: &'static str,
) -> Result<(), GameError> {
    let mut migrator = sqlx::migrate!("./migrations");
    // A stable instance may restart after the candidate has applied a newer additive migration.
    // Known checksums are still verified, while future versions remain intentionally readable.
    migrator.set_ignore_missing(true);
    migrator.run(pool).await.map_err(|error| {
        tracing::error!(%error, operation, "database migration failed");
        GameError::StorageUnavailable
    })
}

fn decode_live_content(value: serde_json::Value) -> Result<LiveContentRevision, GameError> {
    serde_json::from_value(value).map_err(|error| {
        tracing::error!(%error, "stored live-content revision is invalid");
        GameError::Internal
    })
}

fn decode_balance_pin(
    version: i16,
    checksum: &str,
    manifest: serde_json::Value,
) -> Result<BalancePin, GameError> {
    let manifest: BalanceManifest = serde_json::from_value(manifest).map_err(|error| {
        tracing::error!(%error, "stored balance manifest is invalid");
        GameError::Internal
    })?;
    let pin = BalancePin {
        ruleset_version: u16::try_from(version).map_err(|_| GameError::Internal)?,
        checksum: checksum.trim().to_string(),
        manifest,
    };
    if !pin.has_valid_integrity() {
        tracing::error!(
            ruleset_version = version,
            "stored balance pin failed integrity validation"
        );
        return Err(GameError::Internal);
    }
    Ok(pin)
}

fn decode_room_snapshot(
    snapshot: serde_json::Value,
    revision: i64,
    ruleset_version: i16,
    balance_checksum: &str,
) -> Result<GameRoom, GameError> {
    let mut room: GameRoom = serde_json::from_value(snapshot).map_err(|error| {
        tracing::error!(%error, "stored room snapshot is invalid");
        GameError::Internal
    })?;
    room.persistence_revision = revision.max(0) as u64;
    if room.balance.ruleset_version
        != u16::try_from(ruleset_version).map_err(|_| GameError::Internal)?
        || room.balance.checksum != balance_checksum.trim()
        || !room.has_valid_balance_pin()
    {
        tracing::error!(room_id = %room.id, "stored room balance pin disagrees with its index");
        return Err(GameError::Internal);
    }
    Ok(room)
}

async fn persist_safety_relationship(
    transaction: &mut Transaction<'_, Postgres>,
    actor_identity_id: Uuid,
    relationship: &SafetyRelationship,
) -> Result<(), GameError> {
    if relationship.has_effect() {
        sqlx::query(
            "INSERT INTO player_relationships (actor_identity_id,target_identity_id,target_nickname,muted,blocked,updated_at) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (actor_identity_id,target_identity_id) DO UPDATE SET target_nickname=$3,muted=$4,blocked=$5,updated_at=$6",
        )
        .bind(actor_identity_id)
        .bind(relationship.target_identity_id)
        .bind(&relationship.target_nickname)
        .bind(relationship.muted)
        .bind(relationship.blocked)
        .bind(relationship.updated_at)
        .execute(&mut **transaction)
        .await?;
    } else {
        sqlx::query(
            "DELETE FROM player_relationships WHERE actor_identity_id=$1 AND target_identity_id=$2",
        )
        .bind(actor_identity_id)
        .bind(relationship.target_identity_id)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

#[derive(Clone)]
pub struct PostgresRedisStore {
    pool: PgPool,
    cache: Option<ConnectionManager>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseVerification {
    pub migrations_applied: i64,
    pub sessions: i64,
    pub rooms: i64,
    pub results: i64,
    pub matchmaking_entries: i64,
    pub ranked_ratings: i64,
    pub ranked_standings: i64,
    pub ranked_settlements: i64,
    pub ranked_rewards: i64,
    pub ranked_leaderboard_snapshots: i64,
    pub balance_rulesets: i64,
    pub privacy_requests: i64,
    pub deletion_tombstones: i64,
    pub live_content_revisions: i64,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrivacyDeletionTombstone {
    pub account_id: Uuid,
    pub request_id: Uuid,
    pub subject_fingerprint: String,
    pub deleted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrivacyDeletionLedger {
    pub format_version: u8,
    pub generated_at: DateTime<Utc>,
    pub tombstones: Vec<PrivacyDeletionTombstone>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletionLedgerApplyReport {
    pub applied: u64,
    pub already_absent: u64,
    pub remaining_personal_records: i64,
    pub checked_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct LiveContentAuditRow {
    payload: serde_json::Value,
    revision: i64,
    schema_version: i32,
    activate_at: DateTime<Utc>,
    operator_id: String,
    change_note: String,
    rolled_back_from_revision: Option<i64>,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct MatchmakingCandidateRow {
    id: Uuid,
    nickname: String,
    token_hash: String,
    created_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    current_room_id: Option<Uuid>,
    account_id: Option<Uuid>,
    queued_at: DateTime<Utc>,
    pool: String,
    region: String,
    latency_ms: i32,
    rating: Option<i32>,
    season_key: Option<Uuid>,
    party_id: Option<Uuid>,
    party_size: i16,
    recent_pairings: i64,
}

#[derive(sqlx::FromRow)]
struct MatchmakingProfileRow {
    session_id: Uuid,
    pool: String,
    region: String,
    latency_ms: i32,
    rating: Option<i32>,
    season_key: Option<Uuid>,
    party_id: Option<Uuid>,
    party_size: i16,
}

#[derive(sqlx::FromRow)]
struct RankedStandingRow {
    account_id: Uuid,
    rating: i32,
    matches_played: i32,
    wins: i32,
    losses: i32,
    peak_rating: i32,
    last_match_at: Option<DateTime<Utc>>,
    decay_steps_applied: i32,
    season_reward_issued_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct RankedLeaderboardEntryRow {
    rank: i32,
    handle: String,
    rating: i32,
    matches_played: i32,
    wins: i32,
    losses: i32,
    peak_rating: i32,
}

struct StoredMatchmakingCriteria<'a> {
    pool: &'a str,
    region: &'a str,
    latency_ms: i32,
    rating: Option<i32>,
    season_key: Option<Uuid>,
    party_id: Option<Uuid>,
    party_size: i16,
}

fn decode_matchmaking_criteria(
    session_id: Uuid,
    stored: StoredMatchmakingCriteria<'_>,
) -> Result<MatchmakingCriteria, GameError> {
    let pool = MatchmakingPool::from_db_str(stored.pool)?;
    MatchmakingCriteria {
        pool,
        region: MatchmakingRegion::from_db_str(stored.region)?,
        latency_ms: u16::try_from(stored.latency_ms).map_err(|_| GameError::Internal)?,
        rating: stored.rating,
        // A stable server may have inserted an already queued ranked row before the additive
        // migration. It remains readable for restore/drain, but candidate SQL excludes NULL keys
        // so it can never cross a season boundary with a new ticket.
        season_key: stored
            .season_key
            .or((pool == MatchmakingPool::Ranked).then_some(Uuid::nil())),
        party_id: stored.party_id.unwrap_or(session_id),
        party_size: u8::try_from(stored.party_size).map_err(|_| GameError::Internal)?,
    }
    .validate()
    .map_err(|_| GameError::Internal)
}

async fn ensure_ranked_standing(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: Uuid,
    season_id: &str,
    now: DateTime<Utc>,
) -> Result<(), GameError> {
    let previous_rating: Option<i32> = sqlx::query_scalar(
        "SELECT rating FROM ranked_season_standings WHERE account_id=$1 ORDER BY COALESCE(last_match_at,created_at) DESC,updated_at DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(&mut **transaction)
    .await?;
    let seed = next_season_seed(previous_rating);
    sqlx::query(
        "INSERT INTO ranked_season_standings (account_id,season_id,rating,peak_rating,created_at,updated_at) VALUES ($1,$2,$3,$3,$4,$4) ON CONFLICT (account_id,season_id) DO NOTHING",
    )
    .bind(account_id)
    .bind(season_id)
    .bind(seed)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn settle_ranked_match(
    transaction: &mut Transaction<'_, Postgres>,
    room: &GameRoom,
    participant_identities: &[(Uuid, Option<Uuid>)],
) -> Result<(), GameError> {
    let Some(context) = room.ranked_match.as_ref() else {
        return Ok(());
    };
    let Some(result) = room.game.as_ref().and_then(|game| game.result.as_ref()) else {
        return Ok(());
    };
    let inserted = sqlx::query(
        "INSERT INTO ranked_match_settlements (room_id,season_id,settled_at) VALUES ($1,$2,$3) ON CONFLICT (room_id) DO NOTHING",
    )
    .bind(room.id)
    .bind(&context.season_id)
    .bind(result.finished_at)
    .execute(&mut **transaction)
    .await?;
    if inserted.rows_affected() == 0 {
        return Ok(());
    }

    let mut participants = Vec::with_capacity(2);
    for player in &room.players {
        let account_id = participant_identities
            .iter()
            .find(|(session_id, _)| *session_id == player.session_id)
            .and_then(|(_, account_id)| *account_id)
            .ok_or(GameError::Internal)?;
        participants.push((account_id, player.id));
    }
    if participants.len() != 2 || participants[0].0 == participants[1].0 {
        return Err(GameError::Internal);
    }
    participants.sort_by_key(|(account_id, _)| *account_id);
    for (account_id, _) in &participants {
        ensure_ranked_standing(
            transaction,
            *account_id,
            &context.season_id,
            result.finished_at,
        )
        .await?;
    }
    let account_ids: Vec<_> = participants
        .iter()
        .map(|(account_id, _)| *account_id)
        .collect();
    let rows: Vec<RankedStandingRow> = sqlx::query_as(
        "SELECT account_id,rating,matches_played,wins,losses,peak_rating,last_match_at,decay_steps_applied,season_reward_issued_at FROM ranked_season_standings WHERE season_id=$1 AND account_id=ANY($2) ORDER BY account_id FOR UPDATE",
    )
    .bind(&context.season_id)
    .bind(&account_ids)
    .fetch_all(&mut **transaction)
    .await?;
    if rows.len() != 2 {
        return Err(GameError::Internal);
    }
    let record = |row: &RankedStandingRow| -> Result<RankedStandingRecord, GameError> {
        Ok(RankedStandingRecord {
            season_id: context.season_id.clone(),
            rating: row.rating,
            matches_played: u32::try_from(row.matches_played).map_err(|_| GameError::Internal)?,
            wins: u32::try_from(row.wins).map_err(|_| GameError::Internal)?,
            losses: u32::try_from(row.losses).map_err(|_| GameError::Internal)?,
            peak_rating: row.peak_rating,
            last_match_at: row.last_match_at,
            decay_steps_applied: u32::try_from(row.decay_steps_applied)
                .map_err(|_| GameError::Internal)?,
            season_reward_issued_at: row.season_reward_issued_at,
        })
    };
    let mut first = record(&rows[0])?;
    let mut second = record(&rows[1])?;
    let first_player_id = participants
        .iter()
        .find(|(account_id, _)| *account_id == rows[0].account_id)
        .map(|(_, player_id)| *player_id)
        .ok_or(GameError::Internal)?;
    let second_player_id = participants
        .iter()
        .find(|(account_id, _)| *account_id == rows[1].account_id)
        .map(|(_, player_id)| *player_id)
        .ok_or(GameError::Internal)?;
    let first_won = result.winner_id == first_player_id;
    let second_won = result.winner_id == second_player_id;
    if first_won == second_won {
        return Err(GameError::Internal);
    }
    let first_change = first.record_result(second.rating, first_won, result.finished_at);
    let second_change =
        second.record_result(first_change.rating_before, second_won, result.finished_at);

    for (row, standing, change, won) in [
        (&rows[0], first, first_change, first_won),
        (&rows[1], second, second_change, second_won),
    ] {
        sqlx::query(
            "UPDATE ranked_season_standings SET rating=$3,matches_played=$4,wins=$5,losses=$6,peak_rating=$7,last_match_at=$8,decay_steps_applied=0,updated_at=$8 WHERE account_id=$1 AND season_id=$2",
        )
        .bind(row.account_id)
        .bind(&context.season_id)
        .bind(standing.rating)
        .bind(i32::try_from(standing.matches_played).map_err(|_| GameError::Internal)?)
        .bind(i32::try_from(standing.wins).map_err(|_| GameError::Internal)?)
        .bind(i32::try_from(standing.losses).map_err(|_| GameError::Internal)?)
        .bind(standing.peak_rating)
        .bind(result.finished_at)
        .execute(&mut **transaction)
        .await?;
        sqlx::query(
            "INSERT INTO ranked_ratings (account_id,season_id,rating,matches_played,updated_at) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (account_id) DO UPDATE SET season_id=$2,rating=$3,matches_played=$4,updated_at=$5",
        )
        .bind(row.account_id)
        .bind(&context.season_id)
        .bind(standing.rating)
        .bind(i32::try_from(standing.matches_played).map_err(|_| GameError::Internal)?)
        .bind(result.finished_at)
        .execute(&mut **transaction)
        .await?;
        sqlx::query(
            "INSERT INTO ranked_match_participants (room_id,account_id,outcome,rating_before,rating_after,rating_delta,placement_completed) VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(room.id)
        .bind(row.account_id)
        .bind(if won { "WIN" } else { "LOSS" })
        .bind(change.rating_before)
        .bind(change.rating_after)
        .bind(change.delta)
        .bind(change.placement_completed)
        .execute(&mut **transaction)
        .await?;
        sqlx::query(
            "INSERT INTO ranked_reward_ledger (id,account_id,source_kind,source_id,season_id,xp,created_at) VALUES ($1,$2,'RANKED_MATCH',$3,$4,$5,$6) ON CONFLICT (account_id,source_kind,source_id,season_id) DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(row.account_id)
        .bind(room.id.to_string())
        .bind(&context.season_id)
        .bind(i32::try_from(ranked_match_reward_xp(won)).map_err(|_| GameError::Internal)?)
        .bind(result.finished_at)
        .execute(&mut **transaction)
        .await?;
        if change.placement_completed {
            sqlx::query(
                "INSERT INTO ranked_reward_ledger (id,account_id,source_kind,source_id,season_id,xp,created_at) VALUES ($1,$2,'RANKED_PLACEMENT',$3,$3,$4,$5) ON CONFLICT (account_id,source_kind,source_id,season_id) DO NOTHING",
            )
            .bind(Uuid::new_v4())
            .bind(row.account_id)
            .bind(&context.season_id)
            .bind(i32::try_from(ranked_placement_reward_xp()).map_err(|_| GameError::Internal)?)
            .bind(result.finished_at)
            .execute(&mut **transaction)
            .await?;
        }
    }
    Ok(())
}

impl std::fmt::Debug for PostgresRedisStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PostgresRedisStore")
            .finish_non_exhaustive()
    }
}

impl PostgresRedisStore {
    pub async fn migrate_database(database_url: &str) -> Result<(), GameError> {
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(database_url)
            .await?;
        run_compatible_migrations(&pool, "migrate-only").await?;
        pool.close().await;
        Ok(())
    }

    pub async fn verify_database(database_url: &str) -> Result<DatabaseVerification, GameError> {
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(database_url)
            .await?;
        let failed_migrations: i64 =
            sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success IS NOT TRUE")
                .fetch_one(&pool)
                .await?;
        let migrations_applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await?;
        let balance_rows: Vec<(i16, String, serde_json::Value)> = sqlx::query_as(
            "SELECT version,checksum,manifest FROM balance_rulesets ORDER BY version",
        )
        .fetch_all(&pool)
        .await?;
        for (version, checksum, manifest) in &balance_rows {
            decode_balance_pin(*version, checksum, manifest.clone())?;
        }
        let snapshots: Vec<(serde_json::Value, i64, i16, String)> = sqlx::query_as(
            "SELECT snapshot,persistence_revision,ruleset_version,balance_checksum FROM game_rooms",
        )
        .fetch_all(&pool)
        .await?;
        for (snapshot, revision, ruleset_version, balance_checksum) in &snapshots {
            decode_room_snapshot(
                snapshot.clone(),
                *revision,
                *ruleset_version,
                balance_checksum,
            )?;
        }
        let result_balance_rows: Vec<(Uuid, i16, String, serde_json::Value, i16, String)> =
            sqlx::query_as(
                "SELECT result.room_id,result.ruleset_version,result.balance_checksum,result.balance_manifest,room.ruleset_version,room.balance_checksum FROM game_results result JOIN game_rooms room ON room.id=result.room_id",
            )
            .fetch_all(&pool)
            .await?;
        for (
            room_id,
            result_version,
            result_checksum,
            result_manifest,
            room_version,
            room_checksum,
        ) in result_balance_rows
        {
            let pin = decode_balance_pin(result_version, &result_checksum, result_manifest)?;
            let catalog_pin = balance_rows
                .iter()
                .find(|(version, checksum, _)| {
                    *version == result_version && checksum.trim() == result_checksum.trim()
                })
                .map(|(version, checksum, manifest)| {
                    decode_balance_pin(*version, checksum, manifest.clone())
                })
                .transpose()?
                .ok_or(GameError::Internal)?;
            if result_version != room_version
                || result_checksum.trim() != room_checksum.trim()
                || pin.checksum != room_checksum.trim()
                || pin != catalog_pin
            {
                tracing::error!(%room_id, "result and room balance pins disagree");
                return Err(GameError::Internal);
            }
        }
        let matchmaking_rows: Vec<MatchmakingProfileRow> = sqlx::query_as(
            "SELECT session_id,pool,region,latency_ms,rating,season_key,party_id,party_size FROM matchmaking_queue",
        )
        .fetch_all(&pool)
        .await?;
        for row in &matchmaking_rows {
            decode_matchmaking_criteria(
                row.session_id,
                StoredMatchmakingCriteria {
                    pool: &row.pool,
                    region: &row.region,
                    latency_ms: row.latency_ms,
                    rating: row.rating,
                    season_key: row.season_key,
                    party_id: row.party_id,
                    party_size: row.party_size,
                },
            )?;
        }
        let live_content_rows: Vec<LiveContentAuditRow> = sqlx::query_as(
            "SELECT payload,revision,schema_version,activate_at,operator_id,change_note,rolled_back_from_revision,created_at FROM live_content_revisions ORDER BY revision",
        )
        .fetch_all(&pool)
        .await?;
        let live_content_revision_ids: HashSet<i64> =
            live_content_rows.iter().map(|row| row.revision).collect();
        for row in &live_content_rows {
            let decoded = decode_live_content(row.payload.clone())?;
            if decoded.revision != u64::try_from(row.revision).map_err(|_| GameError::Internal)?
                || i32::from(decoded.schema_version) != row.schema_version
                || decoded.activate_at != row.activate_at
                || decoded.operator_id != row.operator_id
                || decoded.change_note != row.change_note
                || decoded
                    .rolled_back_from_revision
                    .map(i64::try_from)
                    .transpose()
                    .map_err(|_| GameError::Internal)?
                    != row.rolled_back_from_revision
                || row.rolled_back_from_revision.is_some_and(|revision| {
                    revision != 0 && !live_content_revision_ids.contains(&revision)
                })
                || decoded.created_at != row.created_at
            {
                return Err(GameError::Internal);
            }
        }
        let broken_references: i64 = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM user_sessions session WHERE session.current_room_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM game_rooms room WHERE room.id=session.current_room_id)) + (SELECT count(*) FROM matchmaking_queue queue WHERE NOT EXISTS (SELECT 1 FROM user_sessions session WHERE session.id=queue.session_id)) + (SELECT count(*) FROM game_result_participants participant WHERE NOT EXISTS (SELECT 1 FROM game_results result WHERE result.room_id=participant.room_id)) + (SELECT count(*) FROM ranked_match_participants participant WHERE NOT EXISTS (SELECT 1 FROM ranked_match_settlements settlement WHERE settlement.room_id=participant.room_id)) + (SELECT count(*) FROM ranked_leaderboard_snapshot_entries entry WHERE NOT EXISTS (SELECT 1 FROM ranked_leaderboard_snapshots snapshot WHERE snapshot.id=entry.snapshot_id) OR NOT EXISTS (SELECT 1 FROM player_accounts account WHERE account.id=entry.account_id)) + (SELECT count(*) FROM ranked_leaderboard_cursors cursor WHERE NOT EXISTS (SELECT 1 FROM ranked_leaderboard_snapshots snapshot WHERE snapshot.id=cursor.snapshot_id))",
        )
        .fetch_one(&pool)
        .await?;
        if failed_migrations != 0 || broken_references != 0 {
            return Err(GameError::Internal);
        }
        let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM user_sessions")
            .fetch_one(&pool)
            .await?;
        let results: i64 = sqlx::query_scalar("SELECT count(*) FROM game_results")
            .fetch_one(&pool)
            .await?;
        let ranked_ratings: i64 = sqlx::query_scalar("SELECT count(*) FROM ranked_ratings")
            .fetch_one(&pool)
            .await?;
        let ranked_standings: i64 =
            sqlx::query_scalar("SELECT count(*) FROM ranked_season_standings")
                .fetch_one(&pool)
                .await?;
        let ranked_settlements: i64 =
            sqlx::query_scalar("SELECT count(*) FROM ranked_match_settlements")
                .fetch_one(&pool)
                .await?;
        let ranked_rewards: i64 = sqlx::query_scalar("SELECT count(*) FROM ranked_reward_ledger")
            .fetch_one(&pool)
            .await?;
        let ranked_leaderboard_snapshots: i64 =
            sqlx::query_scalar("SELECT count(*) FROM ranked_leaderboard_snapshots")
                .fetch_one(&pool)
                .await?;
        let balance_rulesets =
            i64::try_from(balance_rows.len()).map_err(|_| GameError::Internal)?;
        let privacy_requests: i64 = sqlx::query_scalar("SELECT count(*) FROM privacy_requests")
            .fetch_one(&pool)
            .await?;
        let deletion_tombstones: i64 =
            sqlx::query_scalar("SELECT count(*) FROM privacy_deletion_tombstones")
                .fetch_one(&pool)
                .await?;
        let live_content_revisions =
            i64::try_from(live_content_rows.len()).map_err(|_| GameError::Internal)?;
        let resurrected_deletions: i64 = sqlx::query_scalar(DELETION_RESURRECTION_COUNT_QUERY)
            .fetch_one(&pool)
            .await?;
        if resurrected_deletions != 0 {
            return Err(GameError::Internal);
        }
        pool.close().await;
        Ok(DatabaseVerification {
            migrations_applied,
            sessions,
            rooms: snapshots.len() as i64,
            results,
            matchmaking_entries: matchmaking_rows.len() as i64,
            ranked_ratings,
            ranked_standings,
            ranked_settlements,
            ranked_rewards,
            ranked_leaderboard_snapshots,
            balance_rulesets,
            privacy_requests,
            deletion_tombstones,
            live_content_revisions,
            checked_at: Utc::now(),
        })
    }

    pub async fn export_deletion_ledger(
        database_url: &str,
    ) -> Result<PrivacyDeletionLedger, GameError> {
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(database_url)
            .await?;
        let rows: Vec<(Uuid, Uuid, String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT account_id,request_id,subject_fingerprint,deleted_at FROM privacy_deletion_tombstones ORDER BY deleted_at,account_id",
        )
        .fetch_all(&pool)
        .await?;
        pool.close().await;
        Ok(PrivacyDeletionLedger {
            format_version: 1,
            generated_at: Utc::now(),
            tombstones: rows
                .into_iter()
                .map(
                    |(account_id, request_id, subject_fingerprint, deleted_at)| {
                        PrivacyDeletionTombstone {
                            account_id,
                            request_id,
                            subject_fingerprint,
                            deleted_at,
                        }
                    },
                )
                .collect(),
        })
    }

    pub async fn apply_deletion_ledger(
        database_url: &str,
        ledger: PrivacyDeletionLedger,
    ) -> Result<DeletionLedgerApplyReport, GameError> {
        if ledger.format_version != 1 {
            return Err(GameError::InvalidRequest);
        }
        let mut seen_accounts = std::collections::HashSet::new();
        for tombstone in &ledger.tombstones {
            if !seen_accounts.insert(tombstone.account_id)
                || tombstone.subject_fingerprint.len() != 64
                || !tombstone
                    .subject_fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(GameError::InvalidRequest);
            }
        }

        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(database_url)
            .await?;
        run_compatible_migrations(&pool, "deletion-ledger").await?;
        let store = Self { pool, cache: None };
        let mut applied = 0_u64;
        let mut already_absent = 0_u64;
        for tombstone in ledger.tombstones {
            let account_exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM player_accounts WHERE id=$1)")
                    .bind(tombstone.account_id)
                    .fetch_one(&store.pool)
                    .await?;
            if account_exists {
                GameStore::delete_account_data(
                    &store,
                    tombstone.account_id,
                    tombstone.request_id,
                    &tombstone.subject_fingerprint,
                    &[],
                    tombstone.deleted_at,
                    AccountDeletionScope::RestoredBackup,
                )
                .await?;
                applied += 1;
            } else {
                let mut transaction = store.pool.begin().await?;
                sqlx::query(
                    "INSERT INTO privacy_deletion_tombstones (account_id,request_id,subject_fingerprint,deleted_at) VALUES ($1,$2,$3,$4) ON CONFLICT (account_id) DO NOTHING",
                )
                .bind(tombstone.account_id)
                .bind(tombstone.request_id)
                .bind(&tombstone.subject_fingerprint)
                .bind(tombstone.deleted_at)
                .execute(&mut *transaction)
                .await?;
                sqlx::query(
                    "INSERT INTO privacy_requests (id,subject_fingerprint,request_type,status,created_at,completed_at) VALUES ($1,$2,'DELETE','COMPLETED',$3,$3) ON CONFLICT (id) DO NOTHING",
                )
                .bind(tombstone.request_id)
                .bind(&tombstone.subject_fingerprint)
                .bind(tombstone.deleted_at)
                .execute(&mut *transaction)
                .await?;
                transaction.commit().await?;
                already_absent += 1;
            }
        }
        let remaining_personal_records: i64 = sqlx::query_scalar(DELETION_RESURRECTION_COUNT_QUERY)
            .fetch_one(&store.pool)
            .await?;
        if remaining_personal_records != 0 {
            return Err(GameError::Internal);
        }
        store.pool.close().await;
        Ok(DeletionLedgerApplyReport {
            applied,
            already_absent,
            remaining_personal_records,
            checked_at: Utc::now(),
        })
    }

    pub async fn connect(database_url: &str, redis_url: &str) -> Result<Self, GameError> {
        let pool = PgPoolOptions::new()
            .max_connections(16)
            .min_connections(1)
            .connect(database_url)
            .await?;
        run_compatible_migrations(&pool, "server-startup").await?;
        let cache = match redis::Client::open(redis_url) {
            Ok(client) => match tokio::time::timeout(
                REDIS_INITIAL_CONNECT_TIMEOUT,
                ConnectionManager::new(client),
            )
            .await
            {
                Ok(Ok(cache)) => Some(cache),
                Ok(Err(error)) => {
                    tracing::warn!(%error, "redis unavailable; continuing with postgres only");
                    None
                }
                Err(_) => {
                    tracing::warn!(
                        timeout_ms = REDIS_INITIAL_CONNECT_TIMEOUT.as_millis(),
                        "redis connection timed out; continuing with postgres only"
                    );
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
        let row: Option<(serde_json::Value, i64, i16, String)> = sqlx::query_as(
            "SELECT snapshot,persistence_revision,ruleset_version,balance_checksum FROM game_rooms WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|(snapshot, revision, ruleset_version, balance_checksum)| {
            decode_room_snapshot(snapshot, revision, ruleset_version, &balance_checksum)
        })
        .transpose()
    }

    async fn persist_room(
        &self,
        room: &mut GameRoom,
        lease: Option<RoomAuthorityLease>,
    ) -> Result<(), GameError> {
        if !room.has_valid_balance_pin() {
            return Err(GameError::InvalidState);
        }
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
        let ruleset_version =
            i16::try_from(room.balance.ruleset_version).map_err(|_| GameError::Internal)?;
        let result = if let Some(lease) = lease {
            let fencing_token =
                i64::try_from(lease.fencing_token).map_err(|_| GameError::Internal)?;
            sqlx::query(
                "UPDATE game_rooms SET code=$2, name=$3, visibility=$4, status=$5, snapshot=$6, created_at=$7, updated_at=$8, persistence_revision=$9, authority_owner_id=NULL, authority_lease_expires_at=NULL, ruleset_version=$13, balance_checksum=$14 WHERE id=$1 AND persistence_revision=$10 AND authority_owner_id=$11 AND authority_fencing_token=$12 AND authority_lease_expires_at > now()",
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
            .bind(ruleset_version)
            .bind(&room.balance.checksum)
            .execute(&mut *transaction)
            .await?
        } else {
            sqlx::query(
                "INSERT INTO game_rooms (id,code,name,visibility,status,snapshot,created_at,updated_at,persistence_revision,ruleset_version,balance_checksum) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$11,$12) ON CONFLICT (id) DO UPDATE SET name=$3,visibility=$4,status=$5,snapshot=$6,updated_at=$8,persistence_revision=$9,ruleset_version=$11,balance_checksum=$12 WHERE game_rooms.persistence_revision=$10 AND game_rooms.authority_owner_id IS NULL",
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
            .bind(ruleset_version)
            .bind(&room.balance.checksum)
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
            let balance_manifest =
                serde_json::to_value(&room.balance.manifest).map_err(|_| GameError::Internal)?;
            sqlx::query(
                "INSERT INTO game_results (room_id,room_name,participant_session_ids,participant_account_ids,result,finished_at,ruleset_version,balance_checksum,balance_manifest) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (room_id) DO UPDATE SET participant_account_ids=$4,result=$5,finished_at=$6,ruleset_version=$7,balance_checksum=$8,balance_manifest=$9",
            )
            .bind(room.id)
            .bind(&room.name)
            .bind(participant_session_ids)
            .bind(participant_account_ids)
            .bind(result_json)
            .bind(result.finished_at)
            .bind(ruleset_version)
            .bind(&room.balance.checksum)
            .bind(balance_manifest)
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
            settle_ranked_match(&mut transaction, room, &participant_identities).await?;
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

    async fn support_account(
        &self,
        query: &str,
    ) -> Result<Option<SupportAccountSnapshot>, GameError> {
        let row: Option<(Uuid, String, DateTime<Utc>)> =
            if let Ok(account_id) = Uuid::parse_str(query) {
                sqlx::query_as("SELECT id,handle,created_at FROM player_accounts WHERE id=$1")
                    .bind(account_id)
                    .fetch_optional(&self.pool)
                    .await?
            } else {
                sqlx::query_as(
                "SELECT id,handle,created_at FROM player_accounts WHERE lower(handle)=lower($1)",
            )
            .bind(query)
            .fetch_optional(&self.pool)
            .await?
            };
        let Some((id, handle, created_at)) = row else {
            return Ok(None);
        };
        let sessions = self.sessions_for_account(id).await?;
        let rows: Vec<(
            Uuid,
            Uuid,
            String,
            String,
            String,
            Option<Uuid>,
            Vec<Uuid>,
            DateTime<Utc>,
        )> = sqlx::query_as(
            "SELECT id,account_id,operator_id,action_type,reason,target_session_id,affected_session_ids,created_at FROM player_support_actions WHERE account_id=$1 ORDER BY created_at DESC,id DESC LIMIT 100",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        let actions = rows
            .into_iter()
            .map(
                |(
                    id,
                    account_id,
                    operator_id,
                    action_type,
                    reason,
                    target_session_id,
                    affected_session_ids,
                    created_at,
                )| {
                    let action = match action_type.as_str() {
                        "REVOKE_SESSION" => SupportActionKind::RevokeSession,
                        "REVOKE_ALL_SESSIONS" => SupportActionKind::RevokeAllSessions,
                        _ => return Err(GameError::Internal),
                    };
                    Ok(SupportAction {
                        id,
                        account_id,
                        operator_id,
                        action,
                        reason,
                        target_session_id,
                        affected_session_ids,
                        created_at,
                    })
                },
            )
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(SupportAccountSnapshot {
            account: PlayerAccount {
                id,
                handle,
                created_at,
            },
            sessions,
            actions,
        }))
    }

    async fn revoke_account_sessions_for_support(
        &self,
        request: &NewSupportAction,
    ) -> Result<SupportAction, GameError> {
        let mut transaction = self.pool.begin().await?;
        let account: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM player_accounts WHERE id=$1 FOR UPDATE")
                .bind(request.account_id)
                .fetch_optional(&mut *transaction)
                .await?;
        if account.is_none() {
            return Err(GameError::SupportAccountNotFound);
        }
        let affected_session_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM user_sessions WHERE account_id=$1 AND ($2::uuid IS NULL OR id=$2) ORDER BY id FOR UPDATE",
        )
        .bind(request.account_id)
        .bind(request.target_session_id)
        .fetch_all(&mut *transaction)
        .await?;
        if affected_session_ids.is_empty() {
            return Err(GameError::SupportSessionNotFound);
        }
        sqlx::query("DELETE FROM user_sessions WHERE id=ANY($1)")
            .bind(&affected_session_ids)
            .execute(&mut *transaction)
            .await?;
        let action_type = match request.action {
            SupportActionKind::RevokeSession => "REVOKE_SESSION",
            SupportActionKind::RevokeAllSessions => "REVOKE_ALL_SESSIONS",
        };
        sqlx::query(
            "INSERT INTO player_support_actions (id,account_id,operator_id,action_type,reason,target_session_id,affected_session_ids,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(request.id)
        .bind(request.account_id)
        .bind(&request.operator_id)
        .bind(action_type)
        .bind(&request.reason)
        .bind(request.target_session_id)
        .bind(&affected_session_ids)
        .bind(request.created_at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(SupportAction {
            id: request.id,
            account_id: request.account_id,
            operator_id: request.operator_id.clone(),
            action: request.action,
            reason: request.reason.clone(),
            target_session_id: request.target_session_id,
            affected_session_ids,
            created_at: request.created_at,
        })
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
        let leaderboard_visible: bool =
            sqlx::query_scalar("SELECT leaderboard_visible FROM player_accounts WHERE id=$1")
                .bind(account_id)
                .fetch_one(&mut *transaction)
                .await?;
        let mut session_ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM user_sessions WHERE account_id=$1")
                .bind(account_id)
                .fetch_all(&mut *transaction)
                .await?;
        let historical_session_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT DISTINCT session_id FROM game_result_participants WHERE account_id=$1",
        )
        .bind(account_id)
        .fetch_all(&mut *transaction)
        .await?;
        for session_id in historical_session_ids {
            if !session_ids.contains(&session_id) {
                session_ids.push(session_id);
            }
        }
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
        let ranked_rating: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('seasonId',season_id,'rating',rating,'matchesPlayed',matches_played,'updatedAt',updated_at) FROM ranked_ratings WHERE account_id=$1",
        )
        .bind(account_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let ranked_standings: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('seasonId',season_id,'rating',rating,'matchesPlayed',matches_played,'wins',wins,'losses',losses,'peakRating',peak_rating,'lastMatchAt',last_match_at,'decayStepsApplied',decay_steps_applied,'seasonRewardIssuedAt',season_reward_issued_at,'createdAt',created_at,'updatedAt',updated_at) FROM ranked_season_standings WHERE account_id=$1 ORDER BY created_at,season_id",
        )
        .bind(account_id)
        .fetch_all(&mut *transaction)
        .await?;
        let ranked_match_results: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('roomId',participant.room_id,'seasonId',settlement.season_id,'outcome',participant.outcome,'ratingBefore',participant.rating_before,'ratingAfter',participant.rating_after,'ratingDelta',participant.rating_delta,'placementCompleted',participant.placement_completed,'settledAt',settlement.settled_at) FROM ranked_match_participants participant JOIN ranked_match_settlements settlement ON settlement.room_id=participant.room_id WHERE participant.account_id=$1 ORDER BY settlement.settled_at",
        )
        .bind(account_id)
        .fetch_all(&mut *transaction)
        .await?;
        let ranked_rewards: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('sourceKind',source_kind,'sourceId',source_id,'seasonId',season_id,'xp',xp,'createdAt',created_at) FROM ranked_reward_ledger WHERE account_id=$1 ORDER BY created_at",
        )
        .bind(account_id)
        .fetch_all(&mut *transaction)
        .await?;
        let ranked_leaderboard_entries: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('snapshotId',entry.snapshot_id,'seasonId',snapshot.season_id,'rank',entry.rank,'rating',entry.rating,'matchesPlayed',entry.matches_played,'wins',entry.wins,'losses',entry.losses,'peakRating',entry.peak_rating,'generatedAt',snapshot.generated_at,'archived',snapshot.archived) FROM ranked_leaderboard_snapshot_entries entry JOIN ranked_leaderboard_snapshots snapshot ON snapshot.id=entry.snapshot_id WHERE entry.account_id=$1 ORDER BY snapshot.generated_at,entry.rank",
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
            "SELECT jsonb_build_object('id',action.id,'reportId',action.report_id,'action',action.action_type,'reason',action.reason,'expiresAt',action.expires_at,'reversesActionId',action.reverses_action_id,'createdAt',action.created_at) FROM player_moderation_actions action JOIN player_reports report ON report.id=action.report_id WHERE report.reporter_identity_id=ANY($1) OR report.target_identity_id=ANY($1) OR action.target_identity_id=ANY($1) ORDER BY action.created_at",
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
        let support_actions: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('id',id,'operatorId',operator_id,'action',action_type,'reason',reason,'targetSessionId',target_session_id,'affectedSessionIds',affected_session_ids,'createdAt',created_at) FROM player_support_actions WHERE account_id=$1 ORDER BY created_at,id",
        )
        .bind(account_id)
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
            "rankedRating": ranked_rating,
            "rankedStandings": ranked_standings,
            "rankedMatchResults": ranked_match_results,
            "rankedRewards": ranked_rewards,
            "rankedLeaderboardEntries": ranked_leaderboard_entries,
            "leaderboardVisible": leaderboard_visible,
            "safetyRelationships": relationships,
            "moderationReports": reports,
            "moderationActions": moderation_actions,
            "integritySignals": integrity_signals,
            "supportActions": support_actions,
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
        scope: AccountDeletionScope,
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
        let mut session_ids: Vec<_> = sessions.iter().map(|(id, _)| *id).collect();
        let historical_session_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT DISTINCT session_id FROM game_result_participants WHERE account_id=$1",
        )
        .bind(account_id)
        .fetch_all(&mut *transaction)
        .await?;
        for session_id in historical_session_ids {
            if !session_ids.contains(&session_id) {
                session_ids.push(session_id);
            }
        }
        let deleted_names: Vec<_> = std::iter::once(account_handle.clone())
            .chain(sessions.iter().map(|(_, nickname)| nickname.clone()))
            .collect();
        let mut identities = session_ids.clone();
        identities.push(account_id);
        let room_rows: Vec<(Uuid, serde_json::Value, i64, i16, String)> = sqlx::query_as(
            "SELECT id,snapshot,persistence_revision,ruleset_version,balance_checksum FROM game_rooms room WHERE id=ANY($2) OR EXISTS (SELECT 1 FROM jsonb_array_elements(room.snapshot->'players') player WHERE (player->>'sessionId')::uuid=ANY($1)) FOR UPDATE",
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
        for (room_id, snapshot, revision, ruleset_version, balance_checksum) in room_rows {
            let mut room =
                decode_room_snapshot(snapshot, revision, ruleset_version, &balance_checksum)?;
            let deleted_player_ids: Vec<_> = room
                .players
                .iter()
                .filter(|player| session_ids.contains(&player.session_id))
                .map(|player| player.id)
                .collect();
            if !matches!(room.status, RoomStatus::Finished | RoomStatus::Cancelled) {
                if scope == AccountDeletionScope::LiveRequest {
                    return Err(GameError::InvalidState);
                }
                room.status = RoomStatus::Cancelled;
                room.disconnected_deadlines.clear();
            }
            for player in &mut room.players {
                if let Some((_, replacement)) = replacement_session_ids
                    .iter()
                    .find(|(session_id, _)| *session_id == player.session_id)
                {
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
        sqlx::query(
            "UPDATE game_results SET participant_account_ids=array_remove(participant_account_ids,$1) WHERE $1=ANY(participant_account_ids)",
        )
        .bind(account_id)
        .execute(&mut *transaction)
        .await?;
        let rewards_deleted =
            sqlx::query("DELETE FROM progression_reward_ledger WHERE account_id=$1")
                .bind(account_id)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
        let ranked_rewards_deleted =
            sqlx::query("DELETE FROM ranked_reward_ledger WHERE account_id=$1")
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
            "INSERT INTO privacy_deletion_tombstones (account_id,request_id,subject_fingerprint,deleted_at) VALUES ($1,$2,$3,$4) ON CONFLICT (account_id) DO UPDATE SET request_id=EXCLUDED.request_id,subject_fingerprint=EXCLUDED.subject_fingerprint,deleted_at=GREATEST(privacy_deletion_tombstones.deleted_at,EXCLUDED.deleted_at)",
        )
        .bind(account_id)
        .bind(request_id)
        .bind(subject_fingerprint)
        .bind(deleted_at)
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
            rewards_deleted: rewards_deleted.saturating_add(ranked_rewards_deleted),
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
        let row: Option<(serde_json::Value, i64, i16, String)> = sqlx::query_as(
            "SELECT snapshot,persistence_revision,ruleset_version,balance_checksum FROM game_rooms WHERE code=$1",
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await?;
        let room = row
            .map(|(snapshot, revision, ruleset_version, balance_checksum)| {
                decode_room_snapshot(snapshot, revision, ruleset_version, &balance_checksum)
            })
            .transpose()?;
        if let Some(room) = &room {
            self.cache_room(room).await?;
        }
        Ok(room)
    }

    async fn active_rooms(&self) -> Result<Vec<GameRoom>, GameError> {
        let snapshots: Vec<(serde_json::Value, i64, i16, String)> = sqlx::query_as(
            "SELECT snapshot,persistence_revision,ruleset_version,balance_checksum FROM game_rooms WHERE status NOT IN ('FINISHED', 'CANCELLED')",
        )
        .fetch_all(&self.pool)
        .await?;
        snapshots
            .into_iter()
            .map(|(value, revision, ruleset_version, balance_checksum)| {
                decode_room_snapshot(value, revision, ruleset_version, &balance_checksum)
            })
            .collect()
    }

    async fn list_public_rooms(&self) -> Result<Vec<RoomSummary>, GameError> {
        let snapshots: Vec<(serde_json::Value, i64, i16, String)> = sqlx::query_as(
            "SELECT snapshot,persistence_revision,ruleset_version,balance_checksum FROM game_rooms WHERE visibility='PUBLIC' AND status='WAITING_FOR_OPPONENT' ORDER BY created_at DESC LIMIT 100"
        ).fetch_all(&self.pool).await?;
        snapshots
            .into_iter()
            .map(|(value, revision, ruleset_version, balance_checksum)| {
                decode_room_snapshot(value, revision, ruleset_version, &balance_checksum)
                    .map(|room| room.summary())
            })
            .collect()
    }

    async fn list_spectatable_rooms(&self) -> Result<Vec<RoomSummary>, GameError> {
        let snapshots: Vec<(serde_json::Value, i64, i16, String)> = sqlx::query_as(
            "SELECT snapshot,persistence_revision,ruleset_version,balance_checksum FROM game_rooms WHERE visibility='PUBLIC' AND status IN ('PLAYING','FINISHED') ORDER BY updated_at DESC LIMIT 100"
        ).fetch_all(&self.pool).await?;
        snapshots
            .into_iter()
            .map(|(value, revision, ruleset_version, balance_checksum)| {
                decode_room_snapshot(value, revision, ruleset_version, &balance_checksum)
                    .map(|room| room.summary())
            })
            .collect()
    }

    async fn history_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Vec<GameHistoryItem>, GameError> {
        let rows: Vec<(Uuid, String, serde_json::Value, Uuid, i16, String, serde_json::Value)> = sqlx::query_as(
            "WITH identity AS (SELECT account_id FROM user_sessions WHERE id=$1) SELECT results.room_id,results.room_name,results.result,participants.player_id,results.ruleset_version,results.balance_checksum,results.balance_manifest FROM game_results results JOIN game_result_participants participants ON participants.room_id=results.room_id WHERE participants.session_id=$1 OR ((SELECT account_id FROM identity) IS NOT NULL AND participants.account_id=(SELECT account_id FROM identity)) ORDER BY results.finished_at DESC LIMIT 5000"
        ).bind(session_id).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(
                |(room_id, room_name, value, self_player_id, version, checksum, manifest)| {
                    let result = serde_json::from_value(value).map_err(|_| GameError::Internal)?;
                    let balance = decode_balance_pin(version, &checksum, manifest)?;
                    Ok(GameHistoryItem {
                        room_id,
                        room_name,
                        self_player_id,
                        balance,
                        result,
                    })
                },
            )
            .collect()
    }

    async fn enqueue_matchmaking(
        &self,
        session: &UserSession,
        criteria: MatchmakingCriteria,
    ) -> Result<MatchmakingEnqueueResult, GameError> {
        let mut transaction = self.pool.begin().await?;
        let stored_session: Option<(Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
            "SELECT current_room_id, account_id FROM user_sessions WHERE id=$1 AND token_hash=CAST($2 AS CHAR(64)) FOR UPDATE",
        )
        .bind(session.id)
        .bind(&session.token_hash)
        .fetch_optional(&mut *transaction)
        .await?;
        let account_id = match stored_session {
            None => return Err(GameError::Unauthorized),
            Some((Some(_), _)) => return Err(GameError::AlreadyJoined),
            Some((None, account_id)) => account_id,
        };

        let criteria = criteria.validate()?;
        match criteria.pool {
            MatchmakingPool::Casual => {
                if criteria != MatchmakingCriteria::casual(session.id) {
                    return Err(GameError::InvalidRequest);
                }
            }
            MatchmakingPool::Ranked => {
                let account_id = account_id.ok_or(GameError::Unauthorized)?;
                sqlx::query(
                    "INSERT INTO ranked_ratings (account_id) SELECT id FROM player_accounts WHERE id=$1 ON CONFLICT (account_id) DO NOTHING",
                )
                .bind(account_id)
                .execute(&mut *transaction)
                .await?;
                let authoritative_rating: Option<(i32, String)> = sqlx::query_as(
                    "SELECT rating, season_id FROM ranked_ratings WHERE account_id=$1",
                )
                .bind(account_id)
                .fetch_optional(&mut *transaction)
                .await?;
                if criteria.party_id != account_id
                    || authoritative_rating.is_none_or(|(rating, season_id)| {
                        criteria.rating != Some(rating)
                            || criteria.season_key != Some(ranked_season_key(&season_id))
                    })
                {
                    return Err(GameError::InvalidRequest);
                }
            }
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
            "INSERT INTO matchmaking_queue (session_id, queued_at, pool, region, latency_ms, rating, season_key, party_id, party_size) VALUES ($1, now(), $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (session_id) DO NOTHING",
        )
        .bind(session.id)
        .bind(criteria.pool.as_db_str())
        .bind(criteria.region.as_db_str())
        .bind(i32::from(criteria.latency_ms))
        .bind(criteria.rating)
        .bind(criteria.season_key)
        .bind(criteria.party_id)
        .bind(i16::from(criteria.party_size))
        .execute(&mut *transaction)
        .await?;

        let own_row: (
            DateTime<Utc>,
            Option<Uuid>,
            String,
            String,
            i32,
            Option<i32>,
            Option<Uuid>,
            Option<Uuid>,
            i16,
        ) =
            sqlx::query_as(
                "SELECT queued_at, claim_id, pool, region, latency_ms, rating, season_key, party_id, party_size FROM matchmaking_queue WHERE session_id=$1 FOR UPDATE",
            )
        .bind(session.id)
        .fetch_one(&mut *transaction)
        .await?;
        let queued_at = own_row.0;
        let existing_claim = own_row.1;
        let stored_criteria = decode_matchmaking_criteria(
            session.id,
            StoredMatchmakingCriteria {
                pool: &own_row.2,
                region: &own_row.3,
                latency_ms: own_row.4,
                rating: own_row.5,
                season_key: own_row.6,
                party_id: own_row.7,
                party_size: own_row.8,
            },
        )?;
        if stored_criteria != criteria {
            return Err(GameError::InvalidState);
        }
        if existing_claim.is_some() {
            transaction.commit().await?;
            return Ok(MatchmakingEnqueueResult {
                queued_at,
                criteria,
                claim: None,
            });
        }

        let own_identity = account_id.unwrap_or(session.id);
        let opponent: Option<MatchmakingCandidateRow> = sqlx::query_as(
            r#"SELECT sessions.id, sessions.nickname, sessions.token_hash, sessions.created_at,
                sessions.last_seen_at, sessions.current_room_id, sessions.account_id,
                queue.queued_at, queue.pool, queue.region, queue.latency_ms, queue.rating,
                queue.season_key, queue.party_id, queue.party_size, recent.recent_pairings
              FROM matchmaking_queue queue
              JOIN user_sessions sessions ON sessions.id=queue.session_id
              CROSS JOIN LATERAL (
                SELECT count(DISTINCT results.room_id)::bigint AS recent_pairings
                FROM game_results results
                JOIN game_result_participants own_participant
                  ON own_participant.room_id=results.room_id
                JOIN game_result_participants opponent_participant
                  ON opponent_participant.room_id=results.room_id
                 AND opponent_participant.player_id<>own_participant.player_id
                WHERE $3='RANKED'
                  AND results.finished_at >= now()-($11::bigint * interval '1 minute')
                  AND (
                    COALESCE(own_participant.account_id,own_participant.session_id)=$2
                    OR own_participant.session_id IN (
                      SELECT own_session.id FROM user_sessions own_session WHERE own_session.account_id=$2
                    )
                  )
                  AND (
                    COALESCE(opponent_participant.account_id,opponent_participant.session_id)
                      = COALESCE(sessions.account_id,sessions.id)
                    OR opponent_participant.session_id IN (
                      SELECT opponent_session.id
                      FROM user_sessions opponent_session
                      WHERE opponent_session.account_id=COALESCE(sessions.account_id,sessions.id)
                    )
                  )
              ) recent
              WHERE queue.session_id<>$1
                AND queue.claim_id IS NULL
                AND queue.pool=$3
                AND sessions.current_room_id IS NULL
                AND COALESCE(queue.party_id,queue.session_id)<>$4
                AND queue.party_size=$5
                AND ($3='CASUAL' OR queue.season_key=$10)
                AND NOT EXISTS (
                  SELECT 1 FROM player_relationships relationships
                  WHERE relationships.blocked AND (
                    (relationships.actor_identity_id=$2 AND relationships.target_identity_id=COALESCE(sessions.account_id,sessions.id))
                    OR (relationships.actor_identity_id=COALESCE(sessions.account_id,sessions.id) AND relationships.target_identity_id=$2)
                  )
                )
                AND (
                  $3='CASUAL'
                  OR (
                    $6 <= CASE
                      WHEN now()-$9 >= interval '90 seconds' THEN 300
                      WHEN now()-$9 >= interval '30 seconds' THEN 200
                      ELSE 120 END
                    AND queue.latency_ms <= CASE
                      WHEN now()-queue.queued_at >= interval '90 seconds' THEN 300
                      WHEN now()-queue.queued_at >= interval '30 seconds' THEN 200
                      ELSE 120 END
                    AND abs(queue.rating-$7) <= LEAST(
                      CASE WHEN now()-$9 >= interval '90 seconds' THEN 500 WHEN now()-$9 >= interval '30 seconds' THEN 250 ELSE 100 END,
                      CASE WHEN now()-queue.queued_at >= interval '90 seconds' THEN 500 WHEN now()-queue.queued_at >= interval '30 seconds' THEN 250 ELSE 100 END
                    )
                    AND (
                      recent.recent_pairings=0
                      OR (
                        now()-$9 >= interval '90 seconds'
                        AND now()-queue.queued_at >= interval '90 seconds'
                      )
                    )
                    AND (
                      queue.region=$8
                      OR (
                        now()-$9 >= interval '30 seconds'
                        AND now()-queue.queued_at >= interval '30 seconds'
                        AND CASE queue.region
                          WHEN 'KOREA' THEN 'ASIA_PACIFIC'
                          WHEN 'JAPAN' THEN 'ASIA_PACIFIC'
                          WHEN 'SOUTHEAST_ASIA' THEN 'ASIA_PACIFIC'
                          WHEN 'NORTH_AMERICA_WEST' THEN 'NORTH_AMERICA'
                          WHEN 'NORTH_AMERICA_EAST' THEN 'NORTH_AMERICA'
                          WHEN 'EUROPE' THEN 'EUROPE'
                          ELSE 'INVALID' END
                        = CASE $8
                          WHEN 'KOREA' THEN 'ASIA_PACIFIC'
                          WHEN 'JAPAN' THEN 'ASIA_PACIFIC'
                          WHEN 'SOUTHEAST_ASIA' THEN 'ASIA_PACIFIC'
                          WHEN 'NORTH_AMERICA_WEST' THEN 'NORTH_AMERICA'
                          WHEN 'NORTH_AMERICA_EAST' THEN 'NORTH_AMERICA'
                          WHEN 'EUROPE' THEN 'EUROPE'
                          ELSE 'INVALID' END
                      )
                      OR (
                        now()-$9 >= interval '90 seconds'
                        AND now()-queue.queued_at >= interval '90 seconds'
                      )
                    )
                  )
                )
              ORDER BY
                CASE
                  WHEN $3='RANKED'
                    AND LEAST(
                      EXTRACT(EPOCH FROM now()-$9),
                      EXTRACT(EPOCH FROM now()-queue.queued_at)
                    ) < 180
                  THEN recent.recent_pairings
                  ELSE 0
                END ASC,
                queue.queued_at ASC,
                CASE WHEN $3='RANKED' THEN abs(queue.rating-$7) ELSE 0 END ASC,
                GREATEST(queue.latency_ms,$6) ASC
              FOR UPDATE OF queue SKIP LOCKED
              LIMIT 1"#,
        )
        .bind(session.id)
        .bind(own_identity)
        .bind(criteria.pool.as_db_str())
        .bind(criteria.party_id)
        .bind(i16::from(criteria.party_size))
        .bind(i32::from(criteria.latency_ms))
        .bind(criteria.rating)
        .bind(criteria.region.as_db_str())
        .bind(queued_at)
        .bind(criteria.season_key)
        .bind(RECENT_OPPONENT_LOOKBACK_MINUTES)
        .fetch_optional(&mut *transaction)
        .await?;

        let Some(opponent) = opponent else {
            transaction.commit().await?;
            return Ok(MatchmakingEnqueueResult {
                queued_at,
                criteria,
                claim: None,
            });
        };
        let opponent_criteria = decode_matchmaking_criteria(
            opponent.id,
            StoredMatchmakingCriteria {
                pool: &opponent.pool,
                region: &opponent.region,
                latency_ms: opponent.latency_ms,
                rating: opponent.rating,
                season_key: opponent.season_key,
                party_id: opponent.party_id,
                party_size: opponent.party_size,
            },
        )?;
        let quality = matchmaking_quality(
            criteria,
            queued_at,
            opponent_criteria,
            opponent.queued_at,
            Utc::now(),
            u16::try_from(opponent.recent_pairings).unwrap_or(u16::MAX),
        )
        .ok_or(GameError::Internal)?;
        let claim_id = Uuid::new_v4();
        let claimed = sqlx::query(
            "UPDATE matchmaking_queue SET claim_id=$1, claimed_at=now() WHERE session_id=ANY($2) AND claim_id IS NULL",
        )
        .bind(claim_id)
        .bind(vec![session.id, opponent.id])
        .execute(&mut *transaction)
        .await?;
        if claimed.rows_affected() != 2 {
            return Err(GameError::VersionConflict);
        }
        transaction.commit().await?;

        Ok(MatchmakingEnqueueResult {
            queued_at,
            criteria,
            claim: Some(MatchmakingClaim {
                id: claim_id,
                opponent: UserSession {
                    id: opponent.id,
                    account_id: opponent.account_id,
                    nickname: opponent.nickname,
                    token_hash: opponent.token_hash,
                    created_at: opponent.created_at,
                    last_seen_at: opponent.last_seen_at,
                    current_room_id: opponent.current_room_id,
                },
                opponent_queued_at: opponent.queued_at,
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
        if !room.has_valid_balance_pin() || !room.balance.is_registered_for_execution() {
            return Err(GameError::InvalidState);
        }
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
            "INSERT INTO game_rooms (id,code,name,visibility,status,snapshot,created_at,updated_at,persistence_revision,ruleset_version,balance_checksum) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,1,$9,$10) ON CONFLICT (id) DO NOTHING",
        )
        .bind(room.id)
        .bind(&room.code)
        .bind(&room.name)
        .bind(visibility)
        .bind(status)
        .bind(snapshot)
        .bind(room.created_at)
        .bind(room.updated_at)
        .bind(i16::try_from(room.balance.ruleset_version).map_err(|_| GameError::Internal)?)
        .bind(&room.balance.checksum)
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

    async fn latest_live_content(&self) -> Result<Option<LiveContentRevision>, GameError> {
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM live_content_revisions ORDER BY revision DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?
        .map(decode_live_content)
        .transpose()
    }

    async fn active_live_content(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Option<LiveContentRevision>, GameError> {
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM live_content_revisions WHERE activate_at <= $1 ORDER BY revision DESC LIMIT 1",
        )
        .bind(now)
        .fetch_optional(&self.pool)
        .await?
        .map(decode_live_content)
        .transpose()
    }

    async fn live_content_revision(
        &self,
        revision: u64,
    ) -> Result<Option<LiveContentRevision>, GameError> {
        let revision = i64::try_from(revision).map_err(|_| GameError::InvalidRequest)?;
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM live_content_revisions WHERE revision=$1",
        )
        .bind(revision)
        .fetch_optional(&self.pool)
        .await?
        .map(decode_live_content)
        .transpose()
    }

    async fn live_content_history(
        &self,
        limit: usize,
    ) -> Result<Vec<LiveContentRevision>, GameError> {
        let limit = i64::try_from(limit).map_err(|_| GameError::InvalidRequest)?;
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM live_content_revisions ORDER BY revision DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(decode_live_content)
        .collect()
    }

    async fn commit_live_content(
        &self,
        expected_revision: u64,
        candidate: &LiveContentRevision,
    ) -> Result<bool, GameError> {
        let next_revision = expected_revision
            .checked_add(1)
            .ok_or(GameError::InvalidRequest)?;
        if candidate.revision != next_revision {
            return Err(GameError::InvalidRequest);
        }
        let expected_revision =
            i64::try_from(expected_revision).map_err(|_| GameError::InvalidRequest)?;
        let next_revision = i64::try_from(next_revision).map_err(|_| GameError::InvalidRequest)?;
        let payload = serde_json::to_value(candidate).map_err(|error| {
            tracing::error!(%error, "live-content revision serialization failed");
            GameError::Internal
        })?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(LIVE_CONTENT_ADVISORY_LOCK)
            .execute(&mut *transaction)
            .await?;
        let current: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(revision),0) FROM live_content_revisions")
                .fetch_one(&mut *transaction)
                .await?;
        if current != expected_revision {
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO live_content_revisions (revision,schema_version,activate_at,payload,operator_id,change_note,rolled_back_from_revision,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(next_revision)
        .bind(i32::from(candidate.schema_version))
        .bind(candidate.activate_at)
        .bind(payload)
        .bind(&candidate.operator_id)
        .bind(&candidate.change_note)
        .bind(
            candidate
                .rolled_back_from_revision
                .map(i64::try_from)
                .transpose()
                .map_err(|_| GameError::InvalidRequest)?,
        )
        .bind(candidate.created_at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(true)
    }

    async fn identity_for_session(&self, session_id: Uuid) -> Result<Option<Uuid>, GameError> {
        sqlx::query_scalar("SELECT COALESCE(account_id,id) FROM user_sessions WHERE id=$1")
            .bind(session_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Into::into)
    }

    async fn set_safety_relationship(
        &self,
        actor_identity_id: Uuid,
        relationship: SafetyRelationship,
    ) -> Result<(), GameError> {
        let mut transaction = self.pool.begin().await?;
        persist_safety_relationship(&mut transaction, actor_identity_id, &relationship).await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn safety_relationships(
        &self,
        actor_identity_id: Uuid,
    ) -> Result<Vec<SafetyRelationship>, GameError> {
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
                    SafetyRelationship {
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

    async fn safety_relationship_between(
        &self,
        actor_identity_id: Uuid,
        target_identity_id: Uuid,
    ) -> Result<Option<SafetyRelationship>, GameError> {
        let row: Option<(String, bool, bool, DateTime<Utc>)> = sqlx::query_as(
            "SELECT target_nickname,muted,blocked,updated_at FROM player_relationships WHERE actor_identity_id=$1 AND target_identity_id=$2",
        )
        .bind(actor_identity_id)
        .bind(target_identity_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(
            |(target_nickname, muted, blocked, updated_at)| SafetyRelationship {
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

    async fn matchmaking_entry(
        &self,
        session_id: Uuid,
    ) -> Result<Option<MatchmakingQueueEntry>, GameError> {
        let row: Option<(
            DateTime<Utc>,
            String,
            String,
            i32,
            Option<i32>,
            Option<Uuid>,
            Option<Uuid>,
            i16,
        )> =
            sqlx::query_as(
                "SELECT queued_at, pool, region, latency_ms, rating, season_key, party_id, party_size FROM matchmaking_queue WHERE session_id=$1",
            )
            .bind(session_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(
            |(queued_at, pool, region, latency_ms, rating, season_key, party_id, party_size)| {
                Ok(MatchmakingQueueEntry {
                    queued_at,
                    criteria: decode_matchmaking_criteria(
                        session_id,
                        StoredMatchmakingCriteria {
                            pool: &pool,
                            region: &region,
                            latency_ms,
                            rating,
                            season_key,
                            party_id,
                            party_size,
                        },
                    )?,
                })
            },
        )
        .transpose()
    }

    async fn ranked_rating(&self, account_id: Uuid) -> Result<RankedRating, GameError> {
        sqlx::query(
            "INSERT INTO ranked_ratings (account_id) SELECT id FROM player_accounts WHERE id=$1 ON CONFLICT (account_id) DO NOTHING",
        )
        .bind(account_id)
        .execute(&self.pool)
        .await?;
        let row: Option<(i32, i32)> =
            sqlx::query_as("SELECT rating, matches_played FROM ranked_ratings WHERE account_id=$1")
                .bind(account_id)
                .fetch_optional(&self.pool)
                .await?;
        let (rating, matches_played) = row.ok_or(GameError::Unauthorized)?;
        Ok(RankedRating {
            rating,
            matches_played: u32::try_from(matches_played).map_err(|_| GameError::Internal)?,
        })
    }

    async fn ranked_profile(
        &self,
        account_id: Uuid,
        season_id: &str,
        season_starts_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<RankedProfile, GameError> {
        let mut transaction = self.pool.begin().await?;
        let account_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM player_accounts WHERE id=$1)")
                .bind(account_id)
                .fetch_one(&mut *transaction)
                .await?;
        if !account_exists {
            return Err(GameError::Unauthorized);
        }

        let prior_seasons: Vec<(String, i32, i32)> = sqlx::query_as(
            "SELECT season_id,rating,matches_played FROM ranked_season_standings WHERE account_id=$1 AND season_id<>$2 AND matches_played>=5 AND season_reward_issued_at IS NULL AND last_match_at<$3 FOR UPDATE",
        )
        .bind(account_id)
        .bind(season_id)
        .bind(season_starts_at)
        .fetch_all(&mut *transaction)
        .await?;
        for (prior_season_id, rating, matches_played) in prior_seasons {
            let tier = crate::domain::RankedTier::for_standing(
                rating,
                u32::try_from(matches_played).map_err(|_| GameError::Internal)?,
            );
            let reward_xp = tier.season_reward_xp();
            if reward_xp > 0 {
                sqlx::query(
                    "INSERT INTO ranked_reward_ledger (id,account_id,source_kind,source_id,season_id,xp,created_at) VALUES ($1,$2,'RANKED_SEASON',$3,$3,$4,$5) ON CONFLICT (account_id,source_kind,source_id,season_id) DO NOTHING",
                )
                .bind(Uuid::new_v4())
                .bind(account_id)
                .bind(&prior_season_id)
                .bind(i32::try_from(reward_xp).map_err(|_| GameError::Internal)?)
                .bind(now)
                .execute(&mut *transaction)
                .await?;
            }
            sqlx::query(
                "UPDATE ranked_season_standings SET season_reward_issued_at=$3,updated_at=$3 WHERE account_id=$1 AND season_id=$2",
            )
            .bind(account_id)
            .bind(&prior_season_id)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        }

        let previous_rating: Option<i32> = sqlx::query_scalar(
            "SELECT rating FROM ranked_season_standings WHERE account_id=$1 ORDER BY COALESCE(last_match_at,created_at) DESC,updated_at DESC LIMIT 1",
        )
        .bind(account_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let seed_rating = next_season_seed(previous_rating);
        sqlx::query(
            "INSERT INTO ranked_season_standings (account_id,season_id,rating,peak_rating,created_at,updated_at) VALUES ($1,$2,$3,$3,$4,$4) ON CONFLICT (account_id,season_id) DO NOTHING",
        )
        .bind(account_id)
        .bind(season_id)
        .bind(seed_rating)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        let row: (
            i32,
            i32,
            i32,
            i32,
            i32,
            Option<DateTime<Utc>>,
            i32,
            Option<DateTime<Utc>>,
        ) = sqlx::query_as(
            "SELECT rating,matches_played,wins,losses,peak_rating,last_match_at,decay_steps_applied,season_reward_issued_at FROM ranked_season_standings WHERE account_id=$1 AND season_id=$2 FOR UPDATE",
        )
        .bind(account_id)
        .bind(season_id)
        .fetch_one(&mut *transaction)
        .await?;
        let mut standing = RankedStandingRecord {
            season_id: season_id.to_string(),
            rating: row.0,
            matches_played: u32::try_from(row.1).map_err(|_| GameError::Internal)?,
            wins: u32::try_from(row.2).map_err(|_| GameError::Internal)?,
            losses: u32::try_from(row.3).map_err(|_| GameError::Internal)?,
            peak_rating: row.4,
            last_match_at: row.5,
            decay_steps_applied: u32::try_from(row.6).map_err(|_| GameError::Internal)?,
            season_reward_issued_at: row.7,
        };
        standing.apply_inactivity_decay(now);
        sqlx::query(
            "UPDATE ranked_season_standings SET rating=$3,decay_steps_applied=$4,updated_at=$5 WHERE account_id=$1 AND season_id=$2",
        )
        .bind(account_id)
        .bind(season_id)
        .bind(standing.rating)
        .bind(i32::try_from(standing.decay_steps_applied).map_err(|_| GameError::Internal)?)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO ranked_ratings (account_id,season_id,rating,matches_played,updated_at) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (account_id) DO UPDATE SET season_id=$2,rating=$3,matches_played=$4,updated_at=$5",
        )
        .bind(account_id)
        .bind(season_id)
        .bind(standing.rating)
        .bind(i32::try_from(standing.matches_played).map_err(|_| GameError::Internal)?)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        let reward_xp: i64 = sqlx::query_scalar(
            "SELECT COALESCE(sum(xp),0)::bigint FROM ranked_reward_ledger WHERE account_id=$1",
        )
        .bind(account_id)
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(RankedProfile::from_record(
            &standing,
            u64::try_from(reward_xp).map_err(|_| GameError::Internal)?,
        ))
    }

    async fn ranked_leaderboard_visibility(&self, account_id: Uuid) -> Result<bool, GameError> {
        sqlx::query_scalar("SELECT leaderboard_visible FROM player_accounts WHERE id=$1")
            .bind(account_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(GameError::Unauthorized)
    }

    async fn set_ranked_leaderboard_visibility(
        &self,
        account_id: Uuid,
        visible: bool,
    ) -> Result<(), GameError> {
        let updated = sqlx::query("UPDATE player_accounts SET leaderboard_visible=$2 WHERE id=$1")
            .bind(account_id)
            .bind(visible)
            .execute(&self.pool)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(GameError::Unauthorized);
        }
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
        let fetch_limit =
            i64::try_from(limit.saturating_add(1)).map_err(|_| GameError::Internal)?;
        let mut transaction = self.pool.begin().await?;
        if cursor.is_none() && season_id != active_season_id {
            let season_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM ranked_season_standings WHERE season_id=$1 UNION ALL SELECT 1 FROM ranked_leaderboard_snapshots WHERE season_id=$1)",
            )
            .bind(season_id)
            .fetch_one(&mut *transaction)
            .await?;
            if !season_exists {
                return Err(GameError::InvalidRequest);
            }
        }
        sqlx::query("DELETE FROM ranked_leaderboard_cursors WHERE expires_at<=$1")
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "DELETE FROM ranked_leaderboard_snapshots WHERE NOT archived AND expires_at<=$1",
        )
        .bind(now)
        .execute(&mut *transaction)
        .await?;

        let (snapshot_id, generated_at, snapshot_expires_at, snapshot_archived, after_rank) =
            if let Some(cursor_id) = cursor {
                let row: Option<(Uuid, DateTime<Utc>, Option<DateTime<Utc>>, bool, i32)> =
                    sqlx::query_as(
                        "SELECT snapshot.id,snapshot.generated_at,snapshot.expires_at,snapshot.archived,cursor.after_rank FROM ranked_leaderboard_cursors cursor JOIN ranked_leaderboard_snapshots snapshot ON snapshot.id=cursor.snapshot_id WHERE cursor.id=$1 AND cursor.expires_at>$2 AND snapshot.season_id=$3 AND (snapshot.expires_at IS NULL OR snapshot.expires_at>$2)",
                    )
                    .bind(cursor_id)
                    .bind(now)
                    .bind(season_id)
                    .fetch_optional(&mut *transaction)
                    .await?;
                row.ok_or(GameError::InvalidRequest)?
            } else {
                let mut created = false;
                let snapshot_id = if archived {
                    if let Some(existing) = sqlx::query_scalar::<_, Uuid>(
                        "SELECT id FROM ranked_leaderboard_snapshots WHERE season_id=$1 AND archived",
                    )
                    .bind(season_id)
                    .fetch_optional(&mut *transaction)
                    .await?
                    {
                        existing
                    } else {
                        let candidate = Uuid::new_v4();
                        let inserted = sqlx::query_scalar::<_, Uuid>(
                            "INSERT INTO ranked_leaderboard_snapshots (id,season_id,generated_at,expires_at,archived) VALUES ($1,$2,$3,NULL,TRUE) ON CONFLICT DO NOTHING RETURNING id",
                        )
                        .bind(candidate)
                        .bind(season_id)
                        .bind(now)
                        .fetch_optional(&mut *transaction)
                        .await?;
                        if let Some(inserted) = inserted {
                            created = true;
                            inserted
                        } else {
                            sqlx::query_scalar(
                                "SELECT id FROM ranked_leaderboard_snapshots WHERE season_id=$1 AND archived",
                            )
                            .bind(season_id)
                            .fetch_one(&mut *transaction)
                            .await?
                        }
                    }
                } else if let Some(existing) = sqlx::query_scalar::<_, Uuid>(
                    "SELECT id FROM ranked_leaderboard_snapshots WHERE season_id=$1 AND NOT archived AND expires_at>$2 ORDER BY generated_at DESC LIMIT 1",
                )
                .bind(season_id)
                .bind(now)
                .fetch_optional(&mut *transaction)
                .await?
                {
                    existing
                } else {
                    let candidate = Uuid::new_v4();
                    sqlx::query(
                        "INSERT INTO ranked_leaderboard_snapshots (id,season_id,generated_at,expires_at,archived) VALUES ($1,$2,$3,$3+interval '5 minutes',FALSE)",
                    )
                    .bind(candidate)
                    .bind(season_id)
                    .bind(now)
                    .execute(&mut *transaction)
                    .await?;
                    created = true;
                    candidate
                };
                if created {
                    sqlx::query(
                        r#"INSERT INTO ranked_leaderboard_snapshot_entries
                           (snapshot_id,rank,account_id,rating,matches_played,wins,losses,peak_rating)
                           SELECT $1,
                             row_number() OVER (
                               ORDER BY standing.rating DESC,standing.wins DESC,
                                 standing.peak_rating DESC,standing.matches_played ASC,
                                 standing.account_id ASC
                             )::integer,
                             standing.account_id,standing.rating,standing.matches_played,
                             standing.wins,standing.losses,standing.peak_rating
                           FROM ranked_season_standings standing
                           WHERE standing.season_id=$2
                             AND standing.matches_played>=5
                             AND standing.wins+standing.losses=standing.matches_played
                             AND standing.matches_played=(
                               SELECT count(*)::integer
                               FROM ranked_match_participants participant
                               JOIN ranked_match_settlements settlement
                                 ON settlement.room_id=participant.room_id
                               WHERE participant.account_id=standing.account_id
                                 AND settlement.season_id=standing.season_id
                             )"#,
                    )
                    .bind(snapshot_id)
                    .bind(season_id)
                    .execute(&mut *transaction)
                    .await?;
                }
                let row: (DateTime<Utc>, Option<DateTime<Utc>>, bool) = sqlx::query_as(
                    "SELECT generated_at,expires_at,archived FROM ranked_leaderboard_snapshots WHERE id=$1",
                )
                .bind(snapshot_id)
                .fetch_one(&mut *transaction)
                .await?;
                (snapshot_id, row.0, row.1, row.2, 0)
            };

        let mut rows: Vec<RankedLeaderboardEntryRow> = sqlx::query_as(
            r#"SELECT entry.rank,account.handle,entry.rating,entry.matches_played,
                 entry.wins,entry.losses,entry.peak_rating
               FROM ranked_leaderboard_snapshot_entries entry
               JOIN player_accounts account ON account.id=entry.account_id
               WHERE entry.snapshot_id=$1
                 AND entry.rank>$2
                 AND account.leaderboard_visible
                 AND NOT EXISTS (
                   SELECT 1 FROM player_moderation_actions action
                   WHERE (
                     action.target_identity_id=entry.account_id
                     OR action.target_identity_id IN (
                       SELECT session.id FROM user_sessions session
                       WHERE session.account_id=entry.account_id
                     )
                   )
                   AND (
                     action.action_type='BAN'
                     OR (action.action_type='SUSPEND' AND action.expires_at>$3)
                   )
                   AND NOT EXISTS (
                     SELECT 1 FROM player_moderation_actions reversal
                     WHERE reversal.reverses_action_id=action.id
                   )
                 )
               ORDER BY entry.rank
               LIMIT $4"#,
        )
        .bind(snapshot_id)
        .bind(after_rank)
        .bind(now)
        .bind(fetch_limit)
        .fetch_all(&mut *transaction)
        .await?;
        let has_more = rows.len() > limit;
        if has_more {
            rows.truncate(limit);
        }
        let next_cursor = if has_more {
            let after_rank = rows.last().map(|row| row.rank).ok_or(GameError::Internal)?;
            let cursor_id = Uuid::new_v4();
            let cursor_expires_at =
                snapshot_expires_at.unwrap_or(now + chrono::Duration::minutes(15));
            sqlx::query(
                "INSERT INTO ranked_leaderboard_cursors (id,snapshot_id,after_rank,expires_at) VALUES ($1,$2,$3,$4)",
            )
            .bind(cursor_id)
            .bind(snapshot_id)
            .bind(after_rank)
            .bind(cursor_expires_at)
            .execute(&mut *transaction)
            .await?;
            Some(cursor_id)
        } else {
            None
        };

        let mut season_ids: Vec<String> = sqlx::query_scalar(
            "SELECT season_id FROM (SELECT DISTINCT season_id FROM ranked_season_standings UNION SELECT DISTINCT season_id FROM ranked_leaderboard_snapshots) seasons ORDER BY season_id DESC",
        )
        .fetch_all(&mut *transaction)
        .await?;
        if !season_ids
            .iter()
            .any(|candidate| candidate == active_season_id)
        {
            season_ids.insert(0, active_season_id.to_string());
        }
        transaction.commit().await?;

        let entries = rows
            .into_iter()
            .map(|row| {
                let rank = u32::try_from(row.rank).map_err(|_| GameError::Internal)?;
                let matches_played =
                    u32::try_from(row.matches_played).map_err(|_| GameError::Internal)?;
                Ok(RankedLeaderboardEntry {
                    rank,
                    handle: row.handle,
                    rating: row.rating,
                    tier: RankedTier::for_standing(row.rating, matches_played),
                    matches_played,
                    wins: u32::try_from(row.wins).map_err(|_| GameError::Internal)?,
                    losses: u32::try_from(row.losses).map_err(|_| GameError::Internal)?,
                    peak_rating: row.peak_rating,
                })
            })
            .collect::<Result<Vec<_>, GameError>>()?;
        let available_seasons = season_ids
            .into_iter()
            .map(|available_season_id| RankedLeaderboardSeason {
                archived: available_season_id != active_season_id,
                season_id: available_season_id,
            })
            .collect();
        Ok(RankedLeaderboardPage {
            season_id: season_id.to_string(),
            archived: snapshot_archived,
            generated_at,
            entries,
            next_cursor,
            available_seasons,
        })
    }

    async fn matchmaking_queue_stats(&self) -> Result<MatchmakingQueueStats, GameError> {
        let (queued, ranked_queued, oldest_age_seconds): (i64, i64, i64) = sqlx::query_as(
            "SELECT count(*)::bigint, count(*) FILTER (WHERE pool='RANKED')::bigint, COALESCE(EXTRACT(EPOCH FROM now()-min(queued_at)), 0)::bigint FROM matchmaking_queue",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(MatchmakingQueueStats {
            queued: queued.max(0) as u64,
            ranked_queued: ranked_queued.max(0) as u64,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deletion_ledger_rejects_duplicate_accounts_before_database_access() {
        let tombstone = PrivacyDeletionTombstone {
            account_id: Uuid::new_v4(),
            request_id: Uuid::new_v4(),
            subject_fingerprint: "a".repeat(64),
            deleted_at: Utc::now(),
        };
        let ledger = PrivacyDeletionLedger {
            format_version: 1,
            generated_at: Utc::now(),
            tombstones: vec![tombstone.clone(), tombstone],
        };

        assert_eq!(
            PostgresRedisStore::apply_deletion_ledger("postgres://not-contacted", ledger)
                .await
                .unwrap_err(),
            GameError::InvalidRequest
        );
    }
}
