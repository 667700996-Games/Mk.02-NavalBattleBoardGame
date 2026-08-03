use async_trait::async_trait;
use dashmap::DashMap;
use uuid::Uuid;

use crate::{
    domain::{GameRoom, RoomStatus, RoomSummary, RoomVisibility, UserSession},
    error::GameError,
};

use super::{GameHistoryItem, GameStore};

#[derive(Debug, Default)]
pub struct MemoryStore {
    sessions_by_hash: DashMap<String, UserSession>,
    session_hash_by_id: DashMap<Uuid, String>,
    rooms: DashMap<Uuid, GameRoom>,
}

#[async_trait]
impl GameStore for MemoryStore {
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

    async fn save_room(&self, room: &GameRoom) -> Result<(), GameError> {
        self.rooms.insert(room.id, room.clone());
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

    async fn list_public_rooms(&self) -> Result<Vec<RoomSummary>, GameError> {
        let mut rooms: Vec<_> = self
            .rooms
            .iter()
            .filter(|entry| {
                entry.visibility == RoomVisibility::Public
                    && entry.status == RoomStatus::Waiting
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
        let mut history = Vec::new();
        for room in self.rooms.iter() {
            if room
                .players
                .iter()
                .any(|player| player.session_id == session_id)
            {
                if let Some(result) = room.game.as_ref().and_then(|game| game.result.clone()) {
                    history.push(GameHistoryItem {
                        room_id: room.id,
                        room_name: room.name.clone(),
                        result,
                    });
                }
            }
        }
        history.sort_by_key(|item| std::cmp::Reverse(item.result.finished_at));
        Ok(history)
    }

    fn kind(&self) -> &'static str {
        "memory"
    }
}
