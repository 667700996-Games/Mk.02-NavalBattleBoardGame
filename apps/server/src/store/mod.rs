mod memory;
mod postgres;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    domain::{GameResult, GameRoom, RoomSummary, UserSession},
    error::GameError,
};

pub use memory::MemoryStore;
pub use postgres::PostgresRedisStore;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameHistoryItem {
    pub room_id: Uuid,
    pub room_name: String,
    pub self_player_id: Uuid,
    pub result: GameResult,
}

#[derive(Debug, Clone)]
pub struct MatchmakingClaim {
    pub id: Uuid,
    pub opponent: UserSession,
}

#[derive(Debug, Clone)]
pub struct MatchmakingEnqueueResult {
    pub queued_at: DateTime<Utc>,
    pub claim: Option<MatchmakingClaim>,
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
    async fn save_room(&self, room: &mut GameRoom) -> Result<(), GameError>;
    async fn room_by_id(&self, id: Uuid) -> Result<Option<GameRoom>, GameError>;
    async fn room_by_id_authoritative(&self, id: Uuid) -> Result<Option<GameRoom>, GameError> {
        self.room_by_id(id).await
    }
    async fn room_by_code(&self, code: &str) -> Result<Option<GameRoom>, GameError>;
    async fn active_rooms(&self) -> Result<Vec<GameRoom>, GameError>;
    async fn list_public_rooms(&self) -> Result<Vec<RoomSummary>, GameError>;
    async fn history_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Vec<GameHistoryItem>, GameError>;
    async fn enqueue_matchmaking(
        &self,
        session: &UserSession,
    ) -> Result<MatchmakingEnqueueResult, GameError>;
    async fn complete_matchmaking(
        &self,
        claim_id: Uuid,
        room: &mut GameRoom,
    ) -> Result<(), GameError>;
    async fn release_matchmaking_claim(&self, claim_id: Uuid) -> Result<(), GameError>;
    async fn cancel_matchmaking(&self, session_id: Uuid) -> Result<bool, GameError>;
    async fn matchmaking_time(
        &self,
        session_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>, GameError>;
    fn kind(&self) -> &'static str;
}
