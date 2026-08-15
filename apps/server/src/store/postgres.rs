use async_trait::async_trait;
use redis::{AsyncCommands, aio::ConnectionManager};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::{
    domain::{GameRoom, RoomSummary, UserSession},
    error::GameError,
};

use super::{GameHistoryItem, GameStore};

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
            "INSERT INTO user_sessions (id, nickname, token_hash, created_at, last_seen_at, current_room_id) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (id) DO UPDATE SET nickname=$2, token_hash=$3, last_seen_at=$5, current_room_id=$6"
        )
        .bind(session.id).bind(&session.nickname).bind(&session.token_hash)
        .bind(session.created_at).bind(session.last_seen_at).bind(session.current_room_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn session_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<UserSession>, GameError> {
        let row = sqlx::query_as::<_, (Uuid, String, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, Option<Uuid>)>(
            "SELECT id, nickname, token_hash, created_at, last_seen_at, current_room_id FROM user_sessions WHERE token_hash=$1"
        ).bind(token_hash).fetch_optional(&self.pool).await?;
        Ok(row.map(
            |(id, nickname, token_hash, created_at, last_seen_at, current_room_id)| UserSession {
                id,
                nickname,
                token_hash,
                created_at,
                last_seen_at,
                current_room_id,
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

    async fn save_room(&self, room: &mut GameRoom) -> Result<(), GameError> {
        let expected_revision = i64::try_from(room.persistence_revision)
            .map_err(|_| GameError::Internal)?;
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
        let result = sqlx::query(
            "INSERT INTO game_rooms (id, code, name, visibility, status, snapshot, created_at, updated_at, persistence_revision) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (id) DO UPDATE SET name=$3, visibility=$4, status=$5, snapshot=$6, updated_at=$8, persistence_revision=$9 WHERE game_rooms.persistence_revision=$10"
        ).bind(room.id).bind(&room.code).bind(&room.name).bind(visibility).bind(status)
            .bind(snapshot).bind(room.created_at).bind(room.updated_at).bind(next_revision)
            .bind(expected_revision).execute(&mut *transaction).await?;
        if result.rows_affected() == 0 {
            return Err(GameError::VersionConflict);
        }
        if let Some(result) = room.game.as_ref().and_then(|game| game.result.as_ref()) {
            let participant_session_ids: Vec<_> = room
                .players
                .iter()
                .map(|player| player.session_id)
                .collect();
            let result_json = serde_json::to_value(result).map_err(|_| GameError::Internal)?;
            sqlx::query(
                "INSERT INTO game_results (room_id, room_name, participant_session_ids, result, finished_at) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (room_id) DO UPDATE SET result=$4, finished_at=$5"
            ).bind(room.id).bind(&room.name).bind(participant_session_ids).bind(result_json).bind(result.finished_at)
                .execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        room.persistence_revision = persisted.persistence_revision;
        self.cache_room(&persisted).await?;
        Ok(())
    }

    async fn room_by_id(&self, id: Uuid) -> Result<Option<GameRoom>, GameError> {
        if let Some(mut cache) = self.cache.clone() {
            let key = Self::room_cache_key(id);
            match cache.get::<_, Option<String>>(&key).await {
                Ok(Some(cached)) => {
                    if let Ok(room) = serde_json::from_str(&cached) {
                        return Ok(Some(room));
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%error, room_id = %id, "redis cache read skipped");
                }
            }
        }
        let row: Option<(serde_json::Value, i64)> = sqlx::query_as(
            "SELECT snapshot, persistence_revision FROM game_rooms WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let room = row
            .map(|(snapshot, revision)| {
                let mut room: GameRoom =
                    serde_json::from_value(snapshot).map_err(|_| GameError::Internal)?;
                room.persistence_revision = revision.max(0) as u64;
                Ok(room)
            })
            .transpose()?;
        if let Some(room) = &room {
            self.cache_room(room).await?;
        }
        Ok(room)
    }

    async fn room_by_code(&self, code: &str) -> Result<Option<GameRoom>, GameError> {
        let row: Option<(serde_json::Value, i64)> = sqlx::query_as(
            "SELECT snapshot, persistence_revision FROM game_rooms WHERE code=$1",
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await?;
        let room = row
            .map(|(snapshot, revision)| {
                let mut room: GameRoom =
                    serde_json::from_value(snapshot).map_err(|_| GameError::Internal)?;
                room.persistence_revision = revision.max(0) as u64;
                Ok(room)
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
        let rows: Vec<(Uuid, String, serde_json::Value, serde_json::Value)> = sqlx::query_as(
            "SELECT results.room_id, results.room_name, results.result, rooms.snapshot FROM game_results results JOIN game_rooms rooms ON rooms.id = results.room_id WHERE $1 = ANY(results.participant_session_ids) ORDER BY results.finished_at DESC LIMIT 50"
        ).bind(session_id).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|(room_id, room_name, value, snapshot)| {
                let room: GameRoom =
                    serde_json::from_value(snapshot).map_err(|_| GameError::Internal)?;
                let self_player_id = room
                    .players
                    .iter()
                    .find(|player| player.session_id == session_id)
                    .map(|player| player.id)
                    .ok_or(GameError::Internal)?;
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

    fn kind(&self) -> &'static str {
        "postgres+redis"
    }
}
