use std::{sync::Arc, time::Duration};

use chrono::Utc;
use mk01_server::{
    AppState,
    config::{Settings, StorageMode},
    domain::{GameRoom, RoomVisibility, UserSession},
    protocol::{HeartbeatResponse, ServerEvent},
    store::{GameStore, PostgresRedisStore},
};
use tokio::sync::mpsc;
use uuid::Uuid;

fn integration_urls() -> Option<(String, String)> {
    Some((
        std::env::var("TEST_DATABASE_URL").ok()?,
        std::env::var("TEST_REDIS_URL").ok()?,
    ))
}

fn session(nickname: &str) -> UserSession {
    let now = Utc::now();
    UserSession {
        id: Uuid::new_v4(),
        nickname: nickname.to_string(),
        token_hash: Uuid::new_v4().simple().to_string(),
        created_at: now,
        last_seen_at: now,
        current_room_id: None,
    }
}

async fn store(database_url: &str, redis_url: &str) -> Arc<PostgresRedisStore> {
    Arc::new(
        PostgresRedisStore::connect(database_url, redis_url)
            .await
            .expect("postgres and redis integration services must be available"),
    )
}

#[tokio::test]
async fn postgres_fences_stale_writes_and_atomically_completes_distributed_matchmaking() {
    let Some((database_url, redis_url)) = integration_urls() else {
        eprintln!("skipping distributed integration test without TEST_DATABASE_URL/TEST_REDIS_URL");
        return;
    };
    let first_store = store(&database_url, &redis_url).await;
    let second_store = store(&database_url, &redis_url).await;
    let alpha = session("Alpha");
    let bravo = session("Bravo");
    first_store.save_session(&alpha).await.unwrap();
    second_store.save_session(&bravo).await.unwrap();

    let mut authoritative = GameRoom::new(
        Uuid::new_v4().simple().to_string()[..6].to_ascii_uppercase(),
        "Revision fence".to_string(),
        RoomVisibility::Private,
        &alpha,
    )
    .unwrap();
    first_store.save_room(&mut authoritative).await.unwrap();
    let mut first_writer = authoritative.clone();
    let mut stale_writer = authoritative.clone();
    first_writer.name = "First writer".to_string();
    stale_writer.name = "Stale writer".to_string();
    let (first_result, stale_result) = tokio::join!(
        first_store.save_room(&mut first_writer),
        second_store.save_room(&mut stale_writer)
    );
    assert_ne!(first_result.is_ok(), stale_result.is_ok());
    let persisted = first_store
        .room_by_id(authoritative.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.persistence_revision, 2);

    let queued = first_store.enqueue_matchmaking(&alpha).await.unwrap();
    assert!(queued.claim.is_none());
    let matched = second_store.enqueue_matchmaking(&bravo).await.unwrap();
    let claim = matched
        .claim
        .expect("the second player must claim the pair");
    assert_eq!(claim.opponent.id, alpha.id);
    assert!(!first_store.cancel_matchmaking(alpha.id).await.unwrap());

    let mut match_room = GameRoom::new(
        Uuid::new_v4().simple().to_string()[..6].to_ascii_uppercase(),
        "Distributed match".to_string(),
        RoomVisibility::Private,
        &claim.opponent,
    )
    .unwrap();
    match_room.join(&bravo).unwrap();
    second_store
        .complete_matchmaking(claim.id, &mut match_room)
        .await
        .unwrap();
    assert_eq!(match_room.persistence_revision, 1);
    assert_eq!(
        first_store
            .session_by_token_hash(&alpha.token_hash)
            .await
            .unwrap()
            .unwrap()
            .current_room_id,
        Some(match_room.id)
    );
    assert_eq!(
        second_store
            .session_by_token_hash(&bravo.token_hash)
            .await
            .unwrap()
            .unwrap()
            .current_room_id,
        Some(match_room.id)
    );
}

#[tokio::test]
async fn redis_fans_events_out_between_application_instances() {
    let Some((database_url, redis_url)) = integration_urls() else {
        eprintln!("skipping distributed integration test without TEST_DATABASE_URL/TEST_REDIS_URL");
        return;
    };
    let settings = Settings {
        storage_mode: StorageMode::Postgres,
        database_url,
        redis_url,
        distributed_coordination_required: true,
        ..Settings::default()
    };
    let first = AppState::new(settings.clone()).await.unwrap();
    let second = AppState::new(settings).await.unwrap();
    let session_id = Uuid::new_v4();
    let (sender, mut receiver) = mpsc::channel(4);
    second.hub.connect(session_id, sender);

    first
        .send_to_session(
            session_id,
            ServerEvent::Heartbeat(HeartbeatResponse {
                server_time: Utc::now(),
            }),
        )
        .await;
    let payload = tokio::time::timeout(Duration::from_secs(3), receiver.recv())
        .await
        .expect("distributed event delivery timed out")
        .expect("distributed event channel closed");
    let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(value["type"], "heartbeat");
    first.health_check().await.unwrap();
    second.health_check().await.unwrap();
}
