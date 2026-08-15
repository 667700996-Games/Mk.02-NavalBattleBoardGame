mod memory;
mod postgres;

use async_trait::async_trait;
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
    async fn save_room(&self, room: &GameRoom) -> Result<(), GameError>;
    async fn room_by_id(&self, id: Uuid) -> Result<Option<GameRoom>, GameError>;
    async fn room_by_code(&self, code: &str) -> Result<Option<GameRoom>, GameError>;
    async fn active_rooms(&self) -> Result<Vec<GameRoom>, GameError>;
    async fn list_public_rooms(&self) -> Result<Vec<RoomSummary>, GameError>;
    async fn history_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Vec<GameHistoryItem>, GameError>;
    fn kind(&self) -> &'static str;
}
