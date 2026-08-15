use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use std::collections::HashMap;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    domain::{GameRoom, RoomStatus, RoomSummary, RoomVisibility, UserSession},
    error::GameError,
};

use super::{GameHistoryItem, GameStore, MatchmakingClaim, MatchmakingEnqueueResult};

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
        let mut history = Vec::new();
        for room in self.rooms.iter() {
            if let Some(player) = room
                .players
                .iter()
                .find(|player| player.session_id == session_id)
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
        let mut queue = self.matchmaking.lock().await;
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

        let opponent_id = queue
            .iter()
            .filter(|(session_id, entry)| **session_id != session.id && entry.claim_id.is_none())
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
}
