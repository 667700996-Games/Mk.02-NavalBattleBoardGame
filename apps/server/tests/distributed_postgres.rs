use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use mk01_server::{
    AppState, PROTOCOL_VERSION,
    app::hash_token,
    config::{Settings, StorageMode},
    domain::{
        ConnectionState, Coordinate, DEFAULT_RANKED_RATING, GameRoom, LiveContentRevision,
        MatchmakingCriteria, MatchmakingRegion, MatchmakingSearchPhase, Orientation, PlayerAccount,
        RoomStatus, RoomVisibility, ShipKind, ShipPlacement, UserSession, baseline_live_content,
    },
    protocol::{HeartbeatResponse, ServerEvent},
    store::{
        AccountDeletionScope, GameStore, PostgresRedisStore, PrivacyDeletionLedger,
        PrivacyDeletionTombstone,
    },
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
        account_id: None,
        nickname: nickname.to_string(),
        token_hash: Uuid::new_v4().simple().to_string(),
        created_at: now,
        last_seen_at: now,
        current_room_id: None,
    }
}

fn fleet(first_row: u8) -> Vec<ShipPlacement> {
    [
        (ShipKind::Carrier, 0_u8),
        (ShipKind::Battleship, 1),
        (ShipKind::Cruiser, 2),
        (ShipKind::Submarine, 3),
        (ShipKind::Destroyer, 4),
    ]
    .into_iter()
    .map(|(kind, offset)| ShipPlacement {
        kind,
        origin: Coordinate {
            row: first_row + offset,
            col: 0,
        },
        orientation: Orientation::Horizontal,
    })
    .collect()
}

async fn store(database_url: &str, redis_url: &str) -> Arc<PostgresRedisStore> {
    Arc::new(
        PostgresRedisStore::connect(database_url, redis_url)
            .await
            .expect("postgres and redis integration services must be available"),
    )
}

async fn account_session(
    store: &PostgresRedisStore,
    nickname: &str,
) -> (UserSession, PlayerAccount) {
    let guest = session(nickname);
    store.save_session(&guest).await.unwrap();
    let account = PlayerAccount {
        id: Uuid::new_v4(),
        handle: format!("R{}", &Uuid::new_v4().simple().to_string()[..10]),
        created_at: Utc::now(),
    };
    let next_token_hash = Uuid::new_v4().simple().to_string();
    store
        .create_account(
            guest.id,
            &account,
            &hash_token(&Uuid::new_v4().simple().to_string()),
            &next_token_hash,
        )
        .await
        .unwrap();
    let session = store
        .session_by_token_hash(&next_token_hash)
        .await
        .unwrap()
        .unwrap();
    (session, account)
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

    let first_owner = Uuid::new_v4();
    let second_owner = Uuid::new_v4();
    let stale_lease = first_store
        .acquire_room_authority(authoritative.id, first_owner, Duration::from_millis(150))
        .await
        .unwrap()
        .unwrap();
    assert!(
        second_store
            .acquire_room_authority(authoritative.id, second_owner, Duration::from_secs(2),)
            .await
            .unwrap()
            .is_none()
    );
    tokio::time::sleep(Duration::from_millis(180)).await;
    let takeover_lease = second_store
        .acquire_room_authority(authoritative.id, second_owner, Duration::from_secs(2))
        .await
        .unwrap()
        .unwrap();
    assert!(takeover_lease.fencing_token > stale_lease.fencing_token);
    let mut stale_owner_room = persisted.clone();
    stale_owner_room.name = "Stale paused owner".to_string();
    assert_eq!(
        first_store
            .save_room_fenced(&mut stale_owner_room, stale_lease)
            .await
            .unwrap_err(),
        mk01_server::error::GameError::VersionConflict
    );
    let mut takeover_room = persisted.clone();
    takeover_room.name = "Lease takeover".to_string();
    second_store
        .save_room_fenced(&mut takeover_room, takeover_lease)
        .await
        .unwrap();
    assert_eq!(takeover_room.persistence_revision, 3);

    let queued = first_store
        .enqueue_matchmaking(&alpha, MatchmakingCriteria::casual(alpha.id))
        .await
        .unwrap();
    assert!(queued.claim.is_none());
    let matched = second_store
        .enqueue_matchmaking(&bravo, MatchmakingCriteria::casual(bravo.id))
        .await
        .unwrap();
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
async fn postgres_ranked_matchmaking_enforces_authority_and_mutual_widening_across_instances() {
    let Some((database_url, redis_url)) = integration_urls() else {
        eprintln!("skipping distributed integration test without TEST_DATABASE_URL/TEST_REDIS_URL");
        return;
    };
    let first_store = store(&database_url, &redis_url).await;
    let second_store = store(&database_url, &redis_url).await;
    let (first, first_account) = account_session(&first_store, "RankPgAlpha").await;
    let (second, second_account) = account_session(&second_store, "RankPgBravo").await;
    let database = sqlx::PgPool::connect(&database_url).await.unwrap();
    let legacy_casual = session("LegacyCasual");
    first_store.save_session(&legacy_casual).await.unwrap();
    sqlx::query("INSERT INTO matchmaking_queue (session_id,queued_at) VALUES ($1,now())")
        .bind(legacy_casual.id)
        .execute(&database)
        .await
        .unwrap();
    assert_eq!(
        first_store
            .matchmaking_entry(legacy_casual.id)
            .await
            .unwrap()
            .unwrap()
            .criteria,
        MatchmakingCriteria::casual(legacy_casual.id),
        "the additive migration must keep stable-version queue writes readable"
    );
    assert!(
        first_store
            .cancel_matchmaking(legacy_casual.id)
            .await
            .unwrap()
    );
    second_store.ranked_rating(second_account.id).await.unwrap();
    sqlx::query("UPDATE ranked_ratings SET rating=1680, matches_played=12 WHERE account_id=$1")
        .bind(second_account.id)
        .execute(&database)
        .await
        .unwrap();

    let first_criteria = MatchmakingCriteria::ranked(
        first_account.id,
        MatchmakingRegion::Korea,
        80,
        DEFAULT_RANKED_RATING,
    )
    .unwrap();
    let second_criteria =
        MatchmakingCriteria::ranked(second_account.id, MatchmakingRegion::Japan, 90, 1_680)
            .unwrap();
    assert!(
        first_store
            .enqueue_matchmaking(&first, first_criteria)
            .await
            .unwrap()
            .claim
            .is_none()
    );
    assert!(
        second_store
            .enqueue_matchmaking(&second, second_criteria)
            .await
            .unwrap()
            .claim
            .is_none(),
        "exact windows must reject different regional/rating profiles"
    );

    sqlx::query(
        "UPDATE matchmaking_queue SET queued_at=now()-interval '31 seconds' WHERE session_id=ANY($1)",
    )
    .bind(vec![first.id, second.id])
    .execute(&database)
    .await
    .unwrap();
    let matched = second_store
        .enqueue_matchmaking(&second, second_criteria)
        .await
        .unwrap()
        .claim
        .expect("both mutually widened regional windows must match");
    assert_eq!(matched.opponent.id, first.id);
    assert_eq!(matched.quality.phase, MatchmakingSearchPhase::Regional);
    assert_eq!(matched.quality.rating_delta, 180);
    assert_eq!(matched.quality.max_reported_latency_ms, 90);
    second_store
        .release_matchmaking_claim(matched.id)
        .await
        .unwrap();
    assert!(first_store.cancel_matchmaking(first.id).await.unwrap());
    assert!(second_store.cancel_matchmaking(second.id).await.unwrap());

    let spoofed = MatchmakingCriteria::ranked(
        first_account.id,
        MatchmakingRegion::Korea,
        80,
        DEFAULT_RANKED_RATING + 500,
    )
    .unwrap();
    assert_eq!(
        first_store
            .enqueue_matchmaking(&first, spoofed)
            .await
            .unwrap_err(),
        mk01_server::error::GameError::InvalidRequest
    );

    let mut same_party_session = session("RankPgSameParty");
    same_party_session.account_id = Some(first_account.id);
    first_store.save_session(&same_party_session).await.unwrap();
    assert!(
        first_store
            .enqueue_matchmaking(&first, first_criteria)
            .await
            .unwrap()
            .claim
            .is_none()
    );
    assert!(
        second_store
            .enqueue_matchmaking(&same_party_session, first_criteria)
            .await
            .unwrap()
            .claim
            .is_none(),
        "two sessions for one account must not self-match as separate parties"
    );
    assert_eq!(
        first_store
            .matchmaking_queue_stats()
            .await
            .unwrap()
            .ranked_queued,
        2
    );
    assert!(first_store.cancel_matchmaking(first.id).await.unwrap());
    assert!(
        second_store
            .cancel_matchmaking(same_party_session.id)
            .await
            .unwrap()
    );
    let verification = PostgresRedisStore::verify_database(&database_url)
        .await
        .unwrap();
    assert!(verification.ranked_ratings >= 2);
}

#[tokio::test]
async fn postgres_account_export_and_deletion_cover_migrations_and_anonymized_room_state() {
    let Some((database_url, redis_url)) = integration_urls() else {
        eprintln!("skipping distributed integration test without TEST_DATABASE_URL/TEST_REDIS_URL");
        return;
    };
    let store = store(&database_url, &redis_url).await;
    let guest = session("PrivacyCaptain");
    store.save_session(&guest).await.unwrap();
    let account = PlayerAccount {
        id: Uuid::new_v4(),
        handle: "PrivacyCaptain".to_string(),
        created_at: Utc::now(),
    };
    let recovery_key = Uuid::new_v4().simple().to_string();
    let next_token = Uuid::new_v4().simple().to_string();
    store
        .create_account(
            guest.id,
            &account,
            &hash_token(&recovery_key),
            &hash_token(&next_token),
        )
        .await
        .unwrap();
    let account_session = store
        .session_by_token_hash(&hash_token(&next_token))
        .await
        .unwrap()
        .unwrap();

    let mut room = GameRoom::new(
        Uuid::new_v4().simple().to_string()[..6].to_ascii_uppercase(),
        "Postgres privacy operation".to_string(),
        RoomVisibility::Private,
        &account_session,
    )
    .unwrap();
    room.leave(account_session.id).unwrap();
    store.save_room(&mut room).await.unwrap();

    let export_request_id = Uuid::new_v4();
    let archive = store
        .export_account_data(
            account.id,
            export_request_id,
            &hash_token("export-subject"),
            Utc::now(),
        )
        .await
        .unwrap();
    assert_eq!(archive["account"]["id"], account.id.to_string());
    assert_eq!(archive["credentialsExcluded"], true);
    let serialized_archive = archive.to_string();
    assert!(!serialized_archive.contains(&recovery_key));
    assert!(!serialized_archive.contains("token_hash"));

    let delete_request_id = Uuid::new_v4();
    let stats = store
        .delete_account_data(
            account.id,
            delete_request_id,
            &hash_token("delete-subject"),
            &[room.id],
            Utc::now(),
            AccountDeletionScope::LiveRequest,
        )
        .await
        .unwrap();
    assert_eq!(stats.sessions_deleted, 1);
    assert_eq!(stats.rooms_anonymized, 1);
    assert!(
        store
            .account_by_credentials(account.id, &hash_token(&recovery_key))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .session_by_token_hash(&hash_token(&next_token))
            .await
            .unwrap()
            .is_none()
    );
    let anonymized = store
        .room_by_id_authoritative(room.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(anonymized.name, "Archived Operation");
    assert_eq!(anonymized.players[0].nickname, "Deleted Commander");
    assert!(
        anonymized
            .chat_messages
            .iter()
            .all(|message| !message.content.contains(&account.handle))
    );

    let audit_pool = sqlx::PgPool::connect(&database_url).await.unwrap();
    let audit: (String, String) =
        sqlx::query_as("SELECT request_type,status FROM privacy_requests WHERE id=$1")
            .bind(delete_request_id)
            .fetch_one(&audit_pool)
            .await
            .unwrap();
    assert_eq!(audit, ("DELETE".to_string(), "COMPLETED".to_string()));

    let restored_guest = session("RestoreTarget");
    store.save_session(&restored_guest).await.unwrap();
    let restored_account = PlayerAccount {
        id: Uuid::new_v4(),
        handle: "RestoreTarget".to_string(),
        created_at: Utc::now(),
    };
    store
        .create_account(
            restored_guest.id,
            &restored_account,
            &hash_token("restore-recovery"),
            &hash_token("restore-token"),
        )
        .await
        .unwrap();
    let restored_session = store
        .session_by_token_hash(&hash_token("restore-token"))
        .await
        .unwrap()
        .unwrap();
    let mut restored_active_room = GameRoom::new(
        Uuid::new_v4().simple().to_string()[..6].to_ascii_uppercase(),
        "Restored active privacy room".to_string(),
        RoomVisibility::Private,
        &restored_session,
    )
    .unwrap();
    store.save_room(&mut restored_active_room).await.unwrap();
    let restored_request_id = Uuid::new_v4();
    let restored_ledger = PrivacyDeletionLedger {
        format_version: 1,
        generated_at: Utc::now(),
        tombstones: vec![PrivacyDeletionTombstone {
            account_id: restored_account.id,
            request_id: restored_request_id,
            subject_fingerprint: hash_token("restored-delete-subject"),
            deleted_at: Utc::now(),
        }],
    };
    let restored_application =
        PostgresRedisStore::apply_deletion_ledger(&database_url, restored_ledger)
            .await
            .unwrap();
    assert_eq!(restored_application.applied, 1);
    assert_eq!(restored_application.remaining_personal_records, 0);
    let restored_room = store
        .room_by_id_authoritative(restored_active_room.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(restored_room.status, RoomStatus::Cancelled);
    assert_eq!(restored_room.players[0].nickname, "Deleted Commander");

    let deletion_ledger = PostgresRedisStore::export_deletion_ledger(&database_url)
        .await
        .unwrap();
    assert!(
        deletion_ledger
            .tombstones
            .iter()
            .any(|tombstone| tombstone.account_id == account.id)
    );
    let reapplied = PostgresRedisStore::apply_deletion_ledger(&database_url, deletion_ledger)
        .await
        .unwrap();
    assert!(reapplied.already_absent >= 2);
    assert_eq!(reapplied.remaining_personal_records, 0);
    let verification = PostgresRedisStore::verify_database(&database_url)
        .await
        .unwrap();
    assert!(verification.migrations_applied >= 14);
    assert!(verification.deletion_tombstones >= 1);
}

#[tokio::test]
async fn postgres_remains_authoritative_when_the_optional_redis_cache_is_unavailable() {
    let Some((database_url, _)) = integration_urls() else {
        eprintln!("skipping distributed integration test without TEST_DATABASE_URL/TEST_REDIS_URL");
        return;
    };
    let started_at = Instant::now();
    let store = tokio::time::timeout(
        Duration::from_secs(5),
        PostgresRedisStore::connect(&database_url, "redis://127.0.0.1:1/"),
    )
    .await
    .expect("an unavailable optional Redis cache must be abandoned within five seconds")
    .expect("an unavailable optional Redis cache must not disable PostgreSQL authority");
    assert!(started_at.elapsed() < Duration::from_secs(5));
    let captain = session("CacheFailure");
    store.save_session(&captain).await.unwrap();
    let mut room = GameRoom::new(
        Uuid::new_v4().simple().to_string()[..6].to_ascii_uppercase(),
        "Cache failure authority".to_string(),
        RoomVisibility::Private,
        &captain,
    )
    .unwrap();
    store.save_room(&mut room).await.unwrap();

    let recovered = store.room_by_id(room.id).await.unwrap().unwrap();
    let authoritative = store
        .room_by_id_authoritative(room.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.id, room.id);
    assert_eq!(recovered.persistence_revision, room.persistence_revision);
    assert_eq!(
        authoritative.persistence_revision,
        room.persistence_revision
    );

    let optional_settings = Settings {
        storage_mode: StorageMode::Postgres,
        database_url: database_url.clone(),
        redis_url: "redis://127.0.0.1:1/".to_string(),
        distributed_coordination_required: false,
        ..Settings::default()
    };
    let optional_state = tokio::time::timeout(
        Duration::from_secs(6),
        AppState::new(optional_settings.clone()),
    )
    .await
    .expect("optional Redis startup must be abandoned within six seconds")
    .expect("optional Redis loss must permit single-instance PostgreSQL operation");
    optional_state.health_check().await.unwrap();

    let required_result = tokio::time::timeout(
        Duration::from_secs(6),
        AppState::new(Settings {
            distributed_coordination_required: true,
            ..optional_settings
        }),
    )
    .await
    .expect("required Redis startup must fail within six seconds");
    assert!(matches!(
        required_result,
        Err(mk01_server::error::GameError::StorageUnavailable)
    ));
}

#[tokio::test]
async fn postgres_live_content_publish_is_atomic_across_instances_and_respects_activation() {
    let Some((database_url, redis_url)) = integration_urls() else {
        eprintln!("skipping distributed integration test without TEST_DATABASE_URL/TEST_REDIS_URL");
        return;
    };
    let first = store(&database_url, &redis_url).await;
    let second = store(&database_url, &redis_url).await;
    let audit_pool = sqlx::PgPool::connect(&database_url).await.unwrap();
    sqlx::query("DELETE FROM live_content_revisions")
        .execute(&audit_pool)
        .await
        .unwrap();

    let now = Utc::now();
    let baseline = baseline_live_content();
    let first_candidate = LiveContentRevision::from_payload(
        1,
        baseline.payload_for_rollback(now, "First concurrent live publish".into()),
        "liveops-first".into(),
        now,
        None,
    );
    let second_candidate = LiveContentRevision::from_payload(
        1,
        baseline.payload_for_rollback(now, "Second concurrent live publish".into()),
        "liveops-second".into(),
        now,
        None,
    );
    let (first_commit, second_commit) = tokio::join!(
        first.commit_live_content(0, &first_candidate),
        second.commit_live_content(0, &second_candidate)
    );
    assert_ne!(first_commit.unwrap(), second_commit.unwrap());
    assert_eq!(
        first.latest_live_content().await.unwrap().unwrap().revision,
        1
    );

    let scheduled = LiveContentRevision::from_payload(
        2,
        baseline.payload_for_rollback(
            now + chrono::Duration::hours(1),
            "Schedule the next live revision".into(),
        ),
        "liveops-scheduler".into(),
        now,
        None,
    );
    assert!(second.commit_live_content(1, &scheduled).await.unwrap());
    assert_eq!(
        first
            .active_live_content(now)
            .await
            .unwrap()
            .unwrap()
            .revision,
        1
    );
    assert_eq!(
        first
            .active_live_content(now + chrono::Duration::hours(2))
            .await
            .unwrap()
            .unwrap()
            .revision,
        2
    );
    assert_eq!(
        first
            .live_content_history(5)
            .await
            .unwrap()
            .iter()
            .map(|revision| revision.revision)
            .collect::<Vec<_>>(),
        vec![2, 1]
    );
    let rollback_to_baseline = LiveContentRevision::from_payload(
        3,
        baseline.payload_for_rollback(now, "Restore the built-in safe baseline".into()),
        "liveops-rollback".into(),
        now,
        Some(0),
    );
    assert!(
        first
            .commit_live_content(2, &rollback_to_baseline)
            .await
            .unwrap()
    );
    assert_eq!(
        first
            .live_content_revision(3)
            .await
            .unwrap()
            .unwrap()
            .rolled_back_from_revision,
        Some(0)
    );
    let verification = PostgresRedisStore::verify_database(&database_url)
        .await
        .unwrap();
    assert_eq!(verification.live_content_revisions, 3);
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

#[tokio::test]
async fn rolling_instance_replacement_recovers_and_advances_an_active_match() {
    let Some((database_url, redis_url)) = integration_urls() else {
        eprintln!("skipping distributed integration test without TEST_DATABASE_URL/TEST_REDIS_URL");
        return;
    };
    let settings = Settings {
        storage_mode: StorageMode::Postgres,
        database_url,
        redis_url,
        distributed_coordination_required: true,
        reconnect_grace: Duration::from_secs(10),
        turn_duration_seconds: 60,
        ..Settings::default()
    };
    let departing_instance = AppState::new(settings.clone()).await.unwrap();
    let alpha = session("Rolling Alpha");
    let bravo = session("Rolling Bravo");
    departing_instance.store.save_session(&alpha).await.unwrap();
    departing_instance.store.save_session(&bravo).await.unwrap();

    let mut room = GameRoom::new(
        Uuid::new_v4().simple().to_string()[..6].to_ascii_uppercase(),
        "Rolling deployment match".to_string(),
        RoomVisibility::Private,
        &alpha,
    )
    .unwrap();
    room.join(&bravo).unwrap();
    let alpha_player_id = room.player_for_session(alpha.id).unwrap().id;
    let bravo_player_id = room.player_for_session(bravo.id).unwrap().id;
    room.set_lobby_ready(alpha.id, Uuid::new_v4(), alpha_player_id, true)
        .unwrap();
    room.set_lobby_ready(bravo.id, Uuid::new_v4(), bravo_player_id, true)
        .unwrap();
    room.start_placement(alpha.id, Uuid::new_v4(), alpha_player_id, room.version)
        .unwrap();
    room.place_ships(alpha.id, fleet(0)).unwrap();
    room.place_ships(bravo.id, fleet(5)).unwrap();
    room.confirm_placement(alpha.id, &fleet(0), 60).unwrap();
    room.confirm_placement(bravo.id, &fleet(5), 60).unwrap();
    let game_id = room.game_id.unwrap();
    let original_turn = room.game.as_ref().unwrap().turn_number;
    departing_instance.save_room(&mut room).await.unwrap();
    departing_instance
        .store
        .update_session_room(alpha.id, Some(room.id))
        .await
        .unwrap();
    departing_instance
        .store
        .update_session_room(bravo.id, Some(room.id))
        .await
        .unwrap();

    room.disconnect(alpha.id, 10).unwrap();
    departing_instance.save_room(&mut room).await.unwrap();
    let room_id = room.id;
    let recovery_started = std::time::Instant::now();
    drop(departing_instance);

    let replacement = AppState::new(settings).await.unwrap();
    let persisted_alpha = replacement
        .store
        .session_by_token_hash(&alpha.token_hash)
        .await
        .unwrap()
        .unwrap();
    replacement.restore_connection(&persisted_alpha).await;
    let recovered_ref = replacement.room(room_id).await.unwrap();
    let mut recovered = recovered_ref.lock().await;
    assert!(recovery_started.elapsed() < Duration::from_secs(10));
    assert_eq!(recovered.status, RoomStatus::Playing);
    assert_eq!(recovered.game_id, Some(game_id));
    assert_eq!(recovered.game.as_ref().unwrap().turn_number, original_turn);
    assert_eq!(
        recovered
            .player_for_session(alpha.id)
            .unwrap()
            .connection_state,
        ConnectionState::Online
    );
    let snapshot = recovered.snapshot_for(alpha.id).unwrap();
    assert_eq!(snapshot.protocol_version, PROTOCOL_VERSION);
    assert!(snapshot.target_board.is_some());
    assert!(snapshot.revealed_board.is_none());

    let active_player_id = recovered.game.as_ref().unwrap().current_player_id;
    let active_session = recovered
        .players
        .iter()
        .find(|player| player.id == active_player_id)
        .unwrap()
        .session_id;
    let target_row = if active_session == alpha.id { 5 } else { 0 };
    let version = recovered.version;
    recovered
        .fire(
            active_session,
            Uuid::new_v4(),
            active_player_id,
            Coordinate {
                row: target_row,
                col: 0,
            },
            version,
            original_turn,
        )
        .unwrap();
    replacement.save_room(&mut recovered).await.unwrap();
    let committed = replacement
        .store
        .room_by_id_authoritative(room_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(committed.game.as_ref().unwrap().attacks.len(), 1);
}
