use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use mk01_server::{
    AppState, PROTOCOL_VERSION,
    app::hash_token,
    config::{Settings, StorageMode},
    domain::{
        ConnectionState, Coordinate, DEFAULT_RANKED_RATING, FinishReason, Game, GameMode,
        GameResult, GameRoom, LiveContentRevision, MatchmakingCriteria, MatchmakingRegion,
        MatchmakingSearchPhase, NewSupportAction, Orientation, PlayerAccount, PlayerStatistics,
        RankedMatchContext, RankedTier, RoomStatus, RoomVisibility, ShipKind, ShipPlacement,
        SupportActionKind, UserSession, WinType, baseline_live_content, ranked_season_key,
    },
    protocol::{HeartbeatResponse, ServerEvent},
    store::{
        AccountDeletionScope, GameStore, PostgresRedisStore, PrivacyDeletionLedger,
        PrivacyDeletionTombstone,
    },
};
use redis::AsyncCommands;
use tokio::sync::mpsc;
use uuid::Uuid;

fn integration_urls() -> Option<(String, String)> {
    let database_url = std::env::var("TEST_DATABASE_URL").ok();
    let redis_url = std::env::var("TEST_REDIS_URL").ok();
    let required = std::env::var("REQUIRE_DISTRIBUTED_INTEGRATION")
        .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE"));
    assert!(
        !required || (database_url.is_some() && redis_url.is_some()),
        "REQUIRE_DISTRIBUTED_INTEGRATION requires TEST_DATABASE_URL and TEST_REDIS_URL; refusing to skip the service-backed suite"
    );
    database_url.zip(redis_url)
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
async fn postgres_support_actions_revoke_sessions_and_remain_append_only() {
    let Some((database_url, redis_url)) = integration_urls() else {
        eprintln!("skipping distributed integration test without TEST_DATABASE_URL/TEST_REDIS_URL");
        return;
    };
    let store = store(&database_url, &redis_url).await;
    let (session, account) = account_session(&store, "Support database").await;

    let before = store
        .support_account(&account.handle)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.account.id, account.id);
    assert_eq!(before.sessions.len(), 1);
    assert!(before.actions.is_empty());

    let action_id = Uuid::new_v4();
    let action = store
        .revoke_account_sessions_for_support(&NewSupportAction {
            id: action_id,
            account_id: account.id,
            operator_id: "postgres-support-test".to_string(),
            action: SupportActionKind::RevokeSession,
            reason: "Verified compromised device report".to_string(),
            target_session_id: Some(session.id),
            created_at: Utc::now(),
        })
        .await
        .unwrap();
    assert_eq!(action.affected_session_ids, vec![session.id]);
    assert!(
        store
            .session_by_token_hash(&session.token_hash)
            .await
            .unwrap()
            .is_none()
    );
    let after = store
        .support_account(&account.id.to_string())
        .await
        .unwrap()
        .unwrap();
    assert!(after.sessions.is_empty());
    assert_eq!(after.actions.len(), 1);
    assert_eq!(after.actions[0].id, action_id);

    let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
    let update =
        sqlx::query("UPDATE player_support_actions SET reason='tampered history' WHERE id=$1")
            .bind(action_id)
            .execute(&pool)
            .await;
    assert!(
        update.is_err(),
        "support audit rows must reject direct mutation"
    );

    sqlx::query("DELETE FROM player_accounts WHERE id=$1")
        .bind(account.id)
        .execute(&pool)
        .await
        .expect("account privacy cascade must be able to remove support history");
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM player_support_actions WHERE account_id=$1")
            .bind(account.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0);
}

fn finished_ranked_room(first: &UserSession, second: &UserSession, season_id: &str) -> GameRoom {
    let mut room = GameRoom::new(
        Uuid::new_v4().simple().to_string()[..6].to_ascii_uppercase(),
        "Ranked settlement".to_string(),
        RoomVisibility::Private,
        first,
    )
    .unwrap();
    room.join(second).unwrap();
    let winner_id = room.players[0].id;
    let loser_id = room.players[1].id;
    let finished_at = Utc::now();
    room.status = RoomStatus::Finished;
    room.game_id = Some(Uuid::new_v4());
    room.ranked_match = Some(RankedMatchContext {
        season_id: season_id.to_string(),
        content_revision: 0,
    });
    room.game = Some(Game {
        balance: room.balance.clone(),
        boards: HashMap::new(),
        attacks: Vec::new(),
        timeline: Vec::new(),
        first_player_id: winner_id,
        mode: GameMode::Classic,
        shots_remaining_in_turn: 0,
        current_player_id: winner_id,
        turn_number: 1,
        started_at: finished_at - chrono::Duration::minutes(2),
        turn_duration_seconds: 60,
        turn_started_at: None,
        turn_deadline_at: None,
        consecutive_timeout_counts: HashMap::new(),
        total_timeout_counts: HashMap::new(),
        result: Some(GameResult {
            winner_id,
            loser_id,
            total_turns: 1,
            duration_seconds: 120,
            finished_at,
            players: vec![
                PlayerStatistics {
                    player_id: winner_id,
                    shots: 1,
                    hits: 1,
                    ships_sunk: 1,
                    accuracy: 1.0,
                    total_timeouts: 0,
                },
                PlayerStatistics {
                    player_id: loser_id,
                    shots: 1,
                    hits: 0,
                    ships_sunk: 0,
                    accuracy: 0.0,
                    total_timeouts: 0,
                },
            ],
            finish_reason: FinishReason::FleetDestroyed,
            win_type: WinType::NormalVictory,
        }),
    });
    room
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
async fn stable_server_restarts_after_a_future_additive_migration_and_keeps_old_writes_readable() {
    let Some((database_url, redis_url)) = integration_urls() else {
        eprintln!("skipping distributed integration test without TEST_DATABASE_URL/TEST_REDIS_URL");
        return;
    };
    let bootstrap = store(&database_url, &redis_url).await;
    drop(bootstrap);

    let database = sqlx::PgPool::connect(&database_url).await.unwrap();
    let future_version = 209912319999_i64;
    sqlx::query(
        "ALTER TABLE user_sessions ADD COLUMN IF NOT EXISTS future_candidate_metadata JSONB NULL",
    )
    .execute(&database)
    .await
    .unwrap();
    sqlx::query("INSERT INTO _sqlx_migrations (version,description,success,checksum,execution_time) VALUES ($1,'future additive compatibility fixture',true,$2,0)")
        .bind(future_version)
        .bind(vec![0x42_u8; 32])
        .execute(&database)
        .await
        .unwrap();

    let (known_version, known_checksum): (i64, Vec<u8>) = sqlx::query_as(
        "SELECT version,checksum FROM _sqlx_migrations WHERE version<>$1 ORDER BY version LIMIT 1",
    )
    .bind(future_version)
    .fetch_one(&database)
    .await
    .unwrap();
    sqlx::query("UPDATE _sqlx_migrations SET checksum=$2 WHERE version=$1")
        .bind(known_version)
        .bind(vec![0x24_u8; 32])
        .execute(&database)
        .await
        .unwrap();
    assert!(
        PostgresRedisStore::migrate_database(&database_url)
            .await
            .is_err(),
        "known migration checksum drift must still fail closed"
    );
    sqlx::query("UPDATE _sqlx_migrations SET checksum=$2 WHERE version=$1")
        .bind(known_version)
        .bind(known_checksum)
        .execute(&database)
        .await
        .unwrap();

    PostgresRedisStore::migrate_database(&database_url)
        .await
        .expect("migrate-only must tolerate a newer additive schema during rollback");
    let ledger_report = PostgresRedisStore::apply_deletion_ledger(
        &database_url,
        PrivacyDeletionLedger {
            format_version: 1,
            generated_at: Utc::now(),
            tombstones: Vec::new(),
        },
    )
    .await
    .expect("restore replay must tolerate a newer additive schema");
    assert_eq!(ledger_report.remaining_personal_records, 0);

    let stable_store = store(&database_url, &redis_url).await;
    let legacy_session = session("Stable legacy writer");
    sqlx::query("INSERT INTO user_sessions (id,nickname,token_hash,created_at,last_seen_at,current_room_id) VALUES ($1,$2,$3,$4,$4,NULL)")
        .bind(legacy_session.id)
        .bind(&legacy_session.nickname)
        .bind(&legacy_session.token_hash)
        .bind(legacy_session.created_at)
        .execute(&database)
        .await
        .unwrap();
    assert_eq!(
        stable_store
            .session_by_token_hash(&legacy_session.token_hash)
            .await
            .unwrap()
            .unwrap()
            .nickname,
        legacy_session.nickname
    );

    let legacy_opponent = session("Stable result opponent");
    sqlx::query("INSERT INTO user_sessions (id,nickname,token_hash,created_at,last_seen_at,current_room_id) VALUES ($1,$2,$3,$4,$4,NULL)")
        .bind(legacy_opponent.id)
        .bind(&legacy_opponent.nickname)
        .bind(&legacy_opponent.token_hash)
        .bind(legacy_opponent.created_at)
        .execute(&database)
        .await
        .unwrap();
    let mut legacy_finished =
        finished_ranked_room(&legacy_session, &legacy_opponent, "STABLE_RESULT_SEASON");
    legacy_finished.ranked_match = None;
    let legacy_result = legacy_finished
        .game
        .as_ref()
        .and_then(|game| game.result.as_ref())
        .unwrap();
    sqlx::query("INSERT INTO game_rooms (id,code,name,visibility,status,snapshot,created_at,updated_at) VALUES ($1,$2,$3,'PRIVATE','FINISHED',$4,$5,$6)")
        .bind(legacy_finished.id)
        .bind(&legacy_finished.code)
        .bind(&legacy_finished.name)
        .bind(serde_json::to_value(&legacy_finished).unwrap())
        .bind(legacy_finished.created_at)
        .bind(legacy_finished.updated_at)
        .execute(&database)
        .await
        .unwrap();
    let (legacy_trigger_exists, serialized_players): (bool, i32) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM pg_trigger WHERE tgname='game_results_legacy_participant_dual_write'),jsonb_array_length(snapshot->'players') FROM game_rooms WHERE id=$1",
    )
    .bind(legacy_finished.id)
    .fetch_one(&database)
    .await
    .unwrap();
    assert!(legacy_trigger_exists);
    assert_eq!(serialized_players, 2);
    sqlx::query("INSERT INTO game_results (room_id,room_name,participant_session_ids,result,finished_at) VALUES ($1,$2,$3,$4,$5)")
        .bind(legacy_finished.id)
        .bind(&legacy_finished.name)
        .bind(vec![legacy_session.id, legacy_opponent.id])
        .bind(serde_json::to_value(legacy_result).unwrap())
        .bind(legacy_result.finished_at)
        .execute(&database)
        .await
        .unwrap();
    let indexed_participants: i64 =
        sqlx::query_scalar("SELECT count(*) FROM game_result_participants WHERE room_id=$1")
            .bind(legacy_finished.id)
            .fetch_one(&database)
            .await
            .unwrap();
    assert_eq!(indexed_participants, 2);
    assert_eq!(
        stable_store
            .history_for_session(legacy_session.id)
            .await
            .unwrap()
            .len(),
        1
    );
    let mut candidate_room = stable_store
        .room_by_id_authoritative(legacy_finished.id)
        .await
        .unwrap()
        .unwrap();
    candidate_room.name = "Candidate-readable stable result".to_string();
    candidate_room.updated_at = Utc::now();
    stable_store.save_room(&mut candidate_room).await.unwrap();
    let stable_room_projection: (String, serde_json::Value) =
        sqlx::query_as("SELECT name,snapshot FROM game_rooms WHERE id=$1")
            .bind(legacy_finished.id)
            .fetch_one(&database)
            .await
            .unwrap();
    assert_eq!(stable_room_projection.0, candidate_room.name);
    assert!(stable_room_projection.1.is_object());
    let stable_result_projection: (String, Vec<Uuid>, serde_json::Value) = sqlx::query_as(
        "SELECT room_name,participant_session_ids,result FROM game_results WHERE room_id=$1",
    )
    .bind(legacy_finished.id)
    .fetch_one(&database)
    .await
    .unwrap();
    assert_eq!(stable_result_projection.0, legacy_finished.name);
    assert_eq!(stable_result_projection.1.len(), 2);
    assert!(stable_result_projection.2.is_object());

    let candidate_session = session("Candidate writer");
    stable_store.save_session(&candidate_session).await.unwrap();
    let stable_projection: (Uuid, String, Option<Uuid>) =
        sqlx::query_as("SELECT id,nickname,current_room_id FROM user_sessions WHERE id=$1")
            .bind(candidate_session.id)
            .fetch_one(&database)
            .await
            .unwrap();
    assert_eq!(stable_projection.0, candidate_session.id);
    assert_eq!(stable_projection.1, candidate_session.nickname);
    assert_eq!(stable_projection.2, None);

    let applied_migrations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success")
            .fetch_one(&database)
            .await
            .unwrap();
    assert_eq!(
        PostgresRedisStore::verify_database(&database_url)
            .await
            .unwrap()
            .migrations_applied,
        applied_migrations
    );

    drop(stable_store);
    sqlx::query("DELETE FROM game_rooms WHERE id=$1")
        .bind(legacy_finished.id)
        .execute(&database)
        .await
        .unwrap();
    sqlx::query("DELETE FROM user_sessions WHERE id=ANY($1)")
        .bind(vec![
            legacy_session.id,
            legacy_opponent.id,
            candidate_session.id,
        ])
        .execute(&database)
        .await
        .unwrap();
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version=$1")
        .bind(future_version)
        .execute(&database)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE user_sessions DROP COLUMN future_candidate_metadata")
        .execute(&database)
        .await
        .unwrap();
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
    first_store
        .ranked_profile(
            first_account.id,
            "FOUNDERS_SEASON",
            Utc::now() - chrono::Duration::days(1),
            Utc::now(),
        )
        .await
        .unwrap();
    second_store
        .ranked_profile(
            second_account.id,
            "FOUNDERS_SEASON",
            Utc::now() - chrono::Duration::days(1),
            Utc::now(),
        )
        .await
        .unwrap();
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
        ranked_season_key("FOUNDERS_SEASON"),
    )
    .unwrap();
    let second_criteria = MatchmakingCriteria::ranked(
        second_account.id,
        MatchmakingRegion::Japan,
        90,
        1_680,
        ranked_season_key("FOUNDERS_SEASON"),
    )
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
        ranked_season_key("FOUNDERS_SEASON"),
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
async fn postgres_ranked_matchmaking_avoids_recent_opponents_and_relaxes_after_mutual_wait() {
    let Some((database_url, redis_url)) = integration_urls() else {
        eprintln!("skipping distributed integration test without TEST_DATABASE_URL/TEST_REDIS_URL");
        return;
    };
    let store = store(&database_url, &redis_url).await;
    let database = sqlx::PgPool::connect(&database_url).await.unwrap();
    let (first, first_account) = account_session(&store, "FairPgAlpha").await;
    let (recent, recent_account) = account_session(&store, "FairPgBravo").await;
    let (novel, novel_account) = account_session(&store, "FairPgCharlie").await;
    for account in [&first_account, &recent_account, &novel_account] {
        store
            .ranked_profile(
                account.id,
                "FAIRNESS_SEASON",
                Utc::now() - chrono::Duration::days(1),
                Utc::now(),
            )
            .await
            .unwrap();
    }
    let mut previous = finished_ranked_room(&first, &recent, "FAIRNESS_SEASON");
    store.save_room(&mut previous).await.unwrap();

    let first_rating = store.ranked_rating(first_account.id).await.unwrap().rating;
    let recent_rating = store.ranked_rating(recent_account.id).await.unwrap().rating;
    let novel_rating = store.ranked_rating(novel_account.id).await.unwrap().rating;
    let season_key = ranked_season_key("FAIRNESS_SEASON");
    let first_criteria = MatchmakingCriteria::ranked(
        first_account.id,
        MatchmakingRegion::Korea,
        55,
        first_rating,
        season_key,
    )
    .unwrap();
    let recent_criteria = MatchmakingCriteria::ranked(
        recent_account.id,
        MatchmakingRegion::Korea,
        55,
        recent_rating,
        season_key,
    )
    .unwrap();
    let novel_criteria = MatchmakingCriteria::ranked(
        novel_account.id,
        MatchmakingRegion::Korea,
        55,
        novel_rating,
        season_key,
    )
    .unwrap();
    for (session, criteria, seconds) in [
        (&recent, recent_criteria, 100_i64),
        (&novel, novel_criteria, 91_i64),
    ] {
        sqlx::query(
            "INSERT INTO matchmaking_queue (session_id,queued_at,pool,region,latency_ms,rating,season_key,party_id,party_size) VALUES ($1,now()-($2::bigint * interval '1 second'),$3,$4,$5,$6,$7,$8,1)",
        )
        .bind(session.id)
        .bind(seconds)
        .bind(criteria.pool.as_db_str())
        .bind(criteria.region.as_db_str())
        .bind(i32::from(criteria.latency_ms))
        .bind(criteria.rating)
        .bind(criteria.season_key)
        .bind(criteria.party_id)
        .execute(&database)
        .await
        .unwrap();
    }

    let novel_match = store
        .enqueue_matchmaking(&first, first_criteria)
        .await
        .unwrap()
        .claim
        .expect("a novel opponent must be selected instead of the older recent opponent");
    assert_eq!(novel_match.opponent.id, novel.id);
    assert_eq!(novel_match.quality.recent_pairings, 0);
    assert!(!novel_match.quality.rematch_relaxed);
    store
        .release_matchmaking_claim(novel_match.id)
        .await
        .unwrap();
    assert!(store.cancel_matchmaking(first.id).await.unwrap());
    assert!(store.cancel_matchmaking(novel.id).await.unwrap());

    assert!(
        store
            .enqueue_matchmaking(&first, first_criteria)
            .await
            .unwrap()
            .claim
            .is_none(),
        "an exact search must reject the recent opponent"
    );
    sqlx::query(
        "UPDATE matchmaking_queue SET queued_at=now()-interval '91 seconds' WHERE session_id=ANY($1)",
    )
    .bind(vec![first.id, recent.id])
    .execute(&database)
    .await
    .unwrap();
    let relaxed = store
        .enqueue_matchmaking(&first, first_criteria)
        .await
        .unwrap()
        .claim
        .expect("mutual global wait must allow the only available recent opponent");
    assert_eq!(relaxed.opponent.id, recent.id);
    assert_eq!(relaxed.quality.recent_pairings, 1);
    assert!(relaxed.quality.rematch_relaxed);
    assert!(relaxed.quality.shared_wait_seconds >= 90);
    store.release_matchmaking_claim(relaxed.id).await.unwrap();
    assert!(store.cancel_matchmaking(first.id).await.unwrap());
    assert!(store.cancel_matchmaking(recent.id).await.unwrap());
}

#[tokio::test]
async fn postgres_ranked_results_settle_once_complete_placements_and_issue_season_rewards() {
    let Some((database_url, redis_url)) = integration_urls() else {
        eprintln!("skipping distributed integration test without TEST_DATABASE_URL/TEST_REDIS_URL");
        return;
    };
    let store = store(&database_url, &redis_url).await;
    let database = sqlx::PgPool::connect(&database_url).await.unwrap();
    let (first, first_account) = account_session(&store, "RankSettleAlpha").await;
    let (second, second_account) = account_session(&store, "RankSettleBravo").await;
    store
        .ranked_profile(
            first_account.id,
            "SETTLEMENT_SEASON",
            Utc::now() - chrono::Duration::days(1),
            Utc::now(),
        )
        .await
        .unwrap();
    store
        .ranked_profile(
            second_account.id,
            "SETTLEMENT_SEASON",
            Utc::now() - chrono::Duration::days(1),
            Utc::now(),
        )
        .await
        .unwrap();

    let mut first_room = finished_ranked_room(&first, &second, "SETTLEMENT_SEASON");
    store.save_room(&mut first_room).await.unwrap();
    let first_profile = store
        .ranked_profile(
            first_account.id,
            "SETTLEMENT_SEASON",
            Utc::now() - chrono::Duration::days(1),
            Utc::now(),
        )
        .await
        .unwrap();
    let second_profile = store
        .ranked_profile(
            second_account.id,
            "SETTLEMENT_SEASON",
            Utc::now() - chrono::Duration::days(1),
            Utc::now(),
        )
        .await
        .unwrap();
    assert_eq!(first_profile.rating, 1_532);
    assert_eq!(second_profile.rating, 1_468);
    assert_eq!(first_profile.reward_xp_earned, 100);
    assert_eq!(second_profile.reward_xp_earned, 40);

    store.save_room(&mut first_room).await.unwrap();
    let settled_once: i64 =
        sqlx::query_scalar("SELECT count(*) FROM ranked_match_settlements WHERE room_id=$1")
            .bind(first_room.id)
            .fetch_one(&database)
            .await
            .unwrap();
    assert_eq!(settled_once, 1, "a persisted result must never rate twice");

    for _ in 1..5 {
        let mut room = finished_ranked_room(&first, &second, "SETTLEMENT_SEASON");
        store.save_room(&mut room).await.unwrap();
    }
    let placed_first = store
        .ranked_profile(
            first_account.id,
            "SETTLEMENT_SEASON",
            Utc::now() - chrono::Duration::days(1),
            Utc::now(),
        )
        .await
        .unwrap();
    let placed_second = store
        .ranked_profile(
            second_account.id,
            "SETTLEMENT_SEASON",
            Utc::now() - chrono::Duration::days(1),
            Utc::now(),
        )
        .await
        .unwrap();
    assert_eq!(placed_first.placement_matches_remaining, 0);
    assert_eq!(placed_second.placement_matches_remaining, 0);
    assert_ne!(placed_first.tier, RankedTier::Provisional);
    assert_ne!(placed_second.tier, RankedTier::Provisional);
    assert_eq!(placed_first.reward_xp_earned, 1_000);
    assert_eq!(placed_second.reward_xp_earned, 700);

    let next_first = store
        .ranked_profile(
            first_account.id,
            "NEXT_SETTLEMENT_SEASON",
            Utc::now(),
            Utc::now(),
        )
        .await
        .unwrap();
    let next_second = store
        .ranked_profile(
            second_account.id,
            "NEXT_SETTLEMENT_SEASON",
            Utc::now(),
            Utc::now(),
        )
        .await
        .unwrap();
    assert_eq!(next_first.matches_played, 0);
    assert_eq!(next_second.matches_played, 0);
    assert!(next_first.reward_xp_earned > placed_first.reward_xp_earned);
    assert!(next_second.reward_xp_earned > placed_second.reward_xp_earned);
    let season_rewards: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ranked_reward_ledger WHERE source_kind='RANKED_SEASON' AND account_id=ANY($1)",
    )
    .bind(vec![first_account.id, second_account.id])
    .fetch_one(&database)
    .await
    .unwrap();
    assert_eq!(season_rewards, 2);
}

#[tokio::test]
async fn postgres_ranked_leaderboard_uses_authoritative_snapshots_privacy_and_penalties() {
    let Some((database_url, redis_url)) = integration_urls() else {
        eprintln!("skipping distributed integration test without TEST_DATABASE_URL/TEST_REDIS_URL");
        return;
    };
    let store = store(&database_url, &redis_url).await;
    let database = sqlx::PgPool::connect(&database_url).await.unwrap();
    sqlx::query("DELETE FROM player_reports WHERE details='authoritative leaderboard test'")
        .execute(&database)
        .await
        .unwrap();
    sqlx::query(
        "DELETE FROM player_accounts WHERE id IN (SELECT account_id FROM ranked_season_standings WHERE season_id='LEADERBOARD_PG')",
    )
    .execute(&database)
    .await
    .unwrap();
    sqlx::query("DELETE FROM ranked_leaderboard_snapshots WHERE season_id='LEADERBOARD_PG'")
        .execute(&database)
        .await
        .unwrap();
    let (first, first_account) = account_session(&store, "BoardPgAlpha").await;
    let (second, second_account) = account_session(&store, "BoardPgBravo").await;
    let (third, third_account) = account_session(&store, "BoardPgCharlie").await;
    for account in [&first_account, &second_account, &third_account] {
        store
            .ranked_profile(
                account.id,
                "LEADERBOARD_PG",
                Utc::now() - chrono::Duration::days(1),
                Utc::now(),
            )
            .await
            .unwrap();
    }
    for _ in 0..3 {
        let mut first_second = finished_ranked_room(&first, &second, "LEADERBOARD_PG");
        store.save_room(&mut first_second).await.unwrap();
        let mut second_third = finished_ranked_room(&second, &third, "LEADERBOARD_PG");
        store.save_room(&mut second_third).await.unwrap();
        let mut third_first = finished_ranked_room(&third, &first, "LEADERBOARD_PG");
        store.save_room(&mut third_first).await.unwrap();
    }

    let first_page = store
        .ranked_leaderboard(
            "LEADERBOARD_PG",
            "LEADERBOARD_PG",
            false,
            None,
            1,
            Utc::now(),
        )
        .await
        .unwrap();
    assert_eq!(first_page.entries.len(), 1);
    assert_eq!(first_page.entries[0].rank, 1);
    assert!(first_page.next_cursor.is_some());
    let snapshot_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM ranked_leaderboard_snapshots WHERE season_id='LEADERBOARD_PG' AND NOT archived ORDER BY generated_at DESC LIMIT 1",
    )
    .fetch_one(&database)
    .await
    .unwrap();
    let second_rank_account: Uuid = sqlx::query_scalar(
        "SELECT account_id FROM ranked_leaderboard_snapshot_entries WHERE snapshot_id=$1 AND rank=2",
    )
    .bind(snapshot_id)
    .fetch_one(&database)
    .await
    .unwrap();
    let report_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO player_reports (id,reporter_identity_id,target_identity_id,target_nickname,category,details,evidence,status,created_at,updated_at) VALUES ($1,$2,$3,'Ranked subject','CHEATING','authoritative leaderboard test','{}'::jsonb,'ACTIONED',now(),now())",
    )
    .bind(report_id)
    .bind(Uuid::new_v4())
    .bind(second_rank_account)
    .execute(&database)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO player_moderation_actions (id,report_id,target_identity_id,operator_id,action_type,reason,expires_at,created_at) VALUES ($1,$2,$3,'leaderboard-test','SUSPEND','competitive integrity review',now()+interval '1 hour',now())",
    )
    .bind(Uuid::new_v4())
    .bind(report_id)
    .bind(second_rank_account)
    .execute(&database)
    .await
    .unwrap();

    let next_page = store
        .ranked_leaderboard(
            "LEADERBOARD_PG",
            "LEADERBOARD_PG",
            false,
            first_page.next_cursor,
            1,
            Utc::now(),
        )
        .await
        .unwrap();
    assert_eq!(next_page.entries.len(), 1);
    assert_eq!(next_page.entries[0].rank, 3);
    assert!(next_page.next_cursor.is_none());

    let first_rank_account: Uuid = sqlx::query_scalar(
        "SELECT account_id FROM ranked_leaderboard_snapshot_entries WHERE snapshot_id=$1 AND rank=1",
    )
    .bind(snapshot_id)
    .fetch_one(&database)
    .await
    .unwrap();
    store
        .set_ranked_leaderboard_visibility(first_rank_account, false)
        .await
        .unwrap();
    let private_page = store
        .ranked_leaderboard(
            "LEADERBOARD_PG",
            "LEADERBOARD_PG",
            false,
            None,
            10,
            Utc::now(),
        )
        .await
        .unwrap();
    assert_eq!(private_page.entries.len(), 1);
    assert_eq!(private_page.entries[0].rank, 3);

    let archived = store
        .ranked_leaderboard(
            "LEADERBOARD_PG",
            "LEADERBOARD_NEXT",
            true,
            None,
            10,
            Utc::now(),
        )
        .await
        .unwrap();
    sqlx::query(
        "UPDATE ranked_season_standings SET rating=2500,peak_rating=GREATEST(peak_rating,2500) WHERE account_id=$1 AND season_id='LEADERBOARD_PG'",
    )
    .bind(third_account.id)
    .execute(&database)
    .await
    .unwrap();
    let archived_again = store
        .ranked_leaderboard(
            "LEADERBOARD_PG",
            "LEADERBOARD_NEXT",
            true,
            None,
            10,
            Utc::now() + chrono::Duration::minutes(1),
        )
        .await
        .unwrap();
    assert!(archived.archived);
    assert_eq!(archived.generated_at, archived_again.generated_at);
    assert_eq!(archived.entries, archived_again.entries);
    assert_eq!(
        store
            .ranked_leaderboard(
                "LEADERBOARD_PG",
                "LEADERBOARD_NEXT",
                true,
                Some(Uuid::new_v4()),
                10,
                Utc::now(),
            )
            .await
            .unwrap_err(),
        mk01_server::error::GameError::InvalidRequest
    );

    let snapshot_entries_before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ranked_leaderboard_snapshot_entries WHERE account_id=$1",
    )
    .bind(third_account.id)
    .fetch_one(&database)
    .await
    .unwrap();
    assert!(snapshot_entries_before >= 2);
    store
        .delete_account_data(
            third_account.id,
            Uuid::new_v4(),
            &hash_token("leaderboard-delete-subject"),
            &[],
            Utc::now(),
            AccountDeletionScope::LiveRequest,
        )
        .await
        .unwrap();
    let snapshot_entries_after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ranked_leaderboard_snapshot_entries WHERE account_id=$1",
    )
    .bind(third_account.id)
    .fetch_one(&database)
    .await
    .unwrap();
    assert_eq!(snapshot_entries_after, 0);
    assert_eq!(
        store
            .ranked_leaderboard(
                "LEADERBOARD_UNKNOWN",
                "LEADERBOARD_NEXT",
                true,
                None,
                10,
                Utc::now(),
            )
            .await
            .unwrap_err(),
        mk01_server::error::GameError::InvalidRequest
    );
    let unknown_snapshots: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ranked_leaderboard_snapshots WHERE season_id='LEADERBOARD_UNKNOWN'",
    )
    .fetch_one(&database)
    .await
    .unwrap();
    assert_eq!(unknown_snapshots, 0);

    let verification = PostgresRedisStore::verify_database(&database_url)
        .await
        .unwrap();
    assert!(verification.ranked_leaderboard_snapshots >= 2);
}

#[tokio::test]
async fn postgres_balance_catalog_pins_results_and_rejects_mutation() {
    let Some((database_url, redis_url)) = integration_urls() else {
        eprintln!("skipping distributed integration test without TEST_DATABASE_URL/TEST_REDIS_URL");
        return;
    };
    let store = store(&database_url, &redis_url).await;
    let alpha = session("BalancePgAlpha");
    let bravo = session("BalancePgBravo");
    store.save_session(&alpha).await.unwrap();
    store.save_session(&bravo).await.unwrap();
    let mut room = GameRoom::new(
        Uuid::new_v4().simple().to_string()[..6].to_ascii_uppercase(),
        "Pinned postgres operation".to_string(),
        RoomVisibility::Private,
        &alpha,
    )
    .unwrap();
    room.join(&bravo).unwrap();
    let alpha_player = room.player_for_session(alpha.id).unwrap().id;
    let bravo_player = room.player_for_session(bravo.id).unwrap().id;
    room.set_lobby_ready(alpha.id, Uuid::new_v4(), alpha_player, true)
        .unwrap();
    room.set_lobby_ready(bravo.id, Uuid::new_v4(), bravo_player, true)
        .unwrap();
    room.start_placement(alpha.id, Uuid::new_v4(), alpha_player, room.version)
        .unwrap();
    room.place_ships(alpha.id, fleet(0)).unwrap();
    room.place_ships(bravo.id, fleet(5)).unwrap();
    room.confirm_placement(alpha.id, &fleet(0), 60).unwrap();
    room.confirm_placement(bravo.id, &fleet(5), 60).unwrap();
    room.surrender(bravo.id, bravo_player).unwrap();
    let room_id = room.id;
    store.save_room(&mut room).await.unwrap();

    let database = sqlx::PgPool::connect(&database_url).await.unwrap();
    let room_pin: (i16, String) =
        sqlx::query_as("SELECT ruleset_version,balance_checksum FROM game_rooms WHERE id=$1")
            .bind(room_id)
            .fetch_one(&database)
            .await
            .unwrap();
    let result_pin: (i16, String, serde_json::Value) = sqlx::query_as(
        "SELECT ruleset_version,balance_checksum,balance_manifest FROM game_results WHERE room_id=$1",
    )
    .bind(room_id)
    .fetch_one(&database)
    .await
    .unwrap();
    assert_eq!(room_pin.0, 1);
    assert_eq!(room_pin.1.trim(), room.balance.checksum);
    assert_eq!(result_pin.0, room_pin.0);
    assert_eq!(result_pin.1.trim(), room_pin.1.trim());
    assert_eq!(result_pin.2["boardSize"], 10);

    let history = store.history_for_session(alpha.id).await.unwrap();
    assert_eq!(history[0].balance, room.balance);
    let persisted = store
        .room_by_id_authoritative(room_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        persisted.replay_for(alpha.id).unwrap().balance,
        room.balance
    );

    let changed_catalog = sqlx::query(
        "UPDATE balance_rulesets SET change_note='Attempted forbidden rewrite' WHERE version=1",
    )
    .execute(&database)
    .await;
    assert!(changed_catalog.is_err());
    let changed_pin = sqlx::query("UPDATE game_rooms SET balance_checksum=$2 WHERE id=$1")
        .bind(room_id)
        .bind("0".repeat(64))
        .execute(&database)
        .await;
    assert!(changed_pin.is_err());

    let verification = PostgresRedisStore::verify_database(&database_url)
        .await
        .unwrap();
    assert!(verification.balance_rulesets >= 1);
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
    let audit_pool = sqlx::PgPool::connect(&database_url).await.unwrap();
    let leaderboard_snapshot_id = Uuid::new_v4();
    let submitted_report_id = Uuid::new_v4();
    let direct_action_report_id = Uuid::new_v4();
    let direct_action_id = Uuid::new_v4();
    let relationship_target = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO player_accounts (id,handle,recovery_key_hash,created_at) VALUES ($1,'PrivacyPeer',$2,$3)",
    )
    .bind(relationship_target)
    .bind(hash_token("privacy-peer-recovery"))
    .bind(Utc::now())
    .execute(&audit_pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO progression_reward_ledger (id,account_id,source_kind,source_id,period_key,xp) VALUES ($1,$2,'MISSION','PRIVACY_DAILY','2026-08-18',100)")
        .bind(Uuid::new_v4())
        .bind(account.id)
        .execute(&audit_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO ranked_ratings (account_id,rating,matches_played,season_id) VALUES ($1,1530,5,'PRIVACY_SEASON')")
        .bind(account.id)
        .execute(&audit_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO ranked_season_standings (account_id,season_id,rating,matches_played,wins,losses,peak_rating) VALUES ($1,'PRIVACY_SEASON',1530,5,3,2,1540)")
        .bind(account.id)
        .execute(&audit_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO ranked_reward_ledger (id,account_id,source_kind,source_id,season_id,xp) VALUES ($1,$2,'RANKED_SEASON','PRIVACY_REWARD','PRIVACY_SEASON',250)")
        .bind(Uuid::new_v4())
        .bind(account.id)
        .execute(&audit_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO ranked_leaderboard_snapshots (id,season_id,generated_at,expires_at,archived) VALUES ($1,'PRIVACY_SEASON',now(),now()+interval '5 minutes',false)")
        .bind(leaderboard_snapshot_id)
        .execute(&audit_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO ranked_leaderboard_snapshot_entries (snapshot_id,rank,account_id,rating,matches_played,wins,losses,peak_rating) VALUES ($1,1,$2,1530,5,3,2,1540)")
        .bind(leaderboard_snapshot_id)
        .bind(account.id)
        .execute(&audit_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO player_relationships (actor_identity_id,target_identity_id,target_nickname,muted,blocked) VALUES ($1,$2,'Privacy Peer',true,false)")
        .bind(account.id)
        .bind(relationship_target)
        .execute(&audit_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO player_reports (id,reporter_identity_id,target_identity_id,target_nickname,category,details,evidence) VALUES ($1,$2,$3,'Privacy Report Target','OTHER','privacy export fixture','{}')")
        .bind(submitted_report_id)
        .bind(account.id)
        .bind(Uuid::new_v4())
        .execute(&audit_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO player_reports (id,reporter_identity_id,target_identity_id,target_nickname,category,details,evidence) VALUES ($1,$2,$3,'Unrelated Report Target','OTHER','direct action fixture','{}')")
        .bind(direct_action_report_id)
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .execute(&audit_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO player_moderation_actions (id,report_id,target_identity_id,operator_id,action_type,reason) VALUES ($1,$2,$3,'privacy-test-operator','WARN','direct account action fixture')")
        .bind(direct_action_id)
        .bind(direct_action_report_id)
        .bind(account.id)
        .execute(&audit_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO integrity_signals (id,subject_identity_id,kind,severity,confidence,evidence,first_observed_at,last_observed_at) VALUES ($1,$2,'AUTOMATION',3,0.9,'{}',now(),now())")
        .bind(Uuid::new_v4())
        .bind(account.id)
        .execute(&audit_pool)
        .await
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
    let historical_player_id = room.players[0].id;
    sqlx::query("INSERT INTO game_results (room_id,room_name,participant_session_ids,participant_account_ids,result,finished_at) VALUES ($1,$2,$3,$4,'{}',now())")
        .bind(room.id)
        .bind(&room.name)
        .bind(vec![account_session.id])
        .bind(vec![account.id])
        .execute(&audit_pool)
        .await
        .unwrap();
    let indexed_account_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT account_id FROM game_result_participants WHERE room_id=$1 AND player_id=$2",
    )
    .bind(room.id)
    .bind(historical_player_id)
    .fetch_one(&audit_pool)
    .await
    .unwrap();
    assert_eq!(indexed_account_id, Some(account.id));
    store.room_by_id(room.id).await.unwrap().unwrap();
    sqlx::query("DELETE FROM user_sessions WHERE id=$1")
        .bind(account_session.id)
        .execute(&audit_pool)
        .await
        .unwrap();

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
    assert_eq!(archive["sessions"].as_array().unwrap().len(), 0);
    assert_eq!(archive["gameHistory"].as_array().unwrap().len(), 1);
    assert_eq!(archive["progressionRewards"].as_array().unwrap().len(), 1);
    assert!(archive["rankedRating"].is_object());
    assert_eq!(archive["rankedStandings"].as_array().unwrap().len(), 1);
    assert_eq!(archive["rankedRewards"].as_array().unwrap().len(), 1);
    assert_eq!(
        archive["rankedLeaderboardEntries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(archive["safetyRelationships"].as_array().unwrap().len(), 1);
    assert_eq!(archive["moderationReports"].as_array().unwrap().len(), 1);
    assert_eq!(archive["moderationActions"].as_array().unwrap().len(), 1);
    assert_eq!(archive["integritySignals"].as_array().unwrap().len(), 1);
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
            AccountDeletionScope::RestoredBackup,
        )
        .await
        .unwrap();
    assert_eq!(stats.sessions_deleted, 0);
    assert_eq!(stats.rewards_deleted, 2);
    assert_eq!(stats.relationships_deleted, 2);
    assert_eq!(stats.reports_deleted, 1);
    assert_eq!(stats.integrity_signals_deleted, 1);
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
    let redis_client = redis::Client::open(redis_url.as_str()).unwrap();
    let mut redis_connection = redis_client
        .get_multiplexed_async_connection()
        .await
        .unwrap();
    let cached_room_exists: bool = redis_connection
        .exists(format!("mk01:room:{}", room.id))
        .await
        .unwrap();
    assert!(!cached_room_exists);
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
    let result_identities: (Vec<Uuid>, Vec<Uuid>) = sqlx::query_as(
        "SELECT participant_session_ids,participant_account_ids FROM game_results WHERE room_id=$1",
    )
    .bind(room.id)
    .fetch_one(&audit_pool)
    .await
    .unwrap();
    assert!(!result_identities.0.contains(&account_session.id));
    assert!(!result_identities.1.contains(&account.id));
    let participant_identity: (Uuid, Option<Uuid>) = sqlx::query_as(
        "SELECT session_id,account_id FROM game_result_participants WHERE room_id=$1",
    )
    .bind(room.id)
    .fetch_one(&audit_pool)
    .await
    .unwrap();
    assert_ne!(participant_identity.0, account_session.id);
    assert_eq!(participant_identity.1, None);
    let removed_derived_records: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM ranked_ratings WHERE account_id=$1) + (SELECT count(*) FROM ranked_season_standings WHERE account_id=$1) + (SELECT count(*) FROM ranked_reward_ledger WHERE account_id=$1) + (SELECT count(*) FROM progression_reward_ledger WHERE account_id=$1) + (SELECT count(*) FROM ranked_leaderboard_snapshot_entries WHERE account_id=$1) + (SELECT count(*) FROM player_relationships WHERE actor_identity_id=$1 OR target_identity_id=$1) + (SELECT count(*) FROM player_reports WHERE reporter_identity_id=$1 OR target_identity_id=$1) + (SELECT count(*) FROM player_moderation_actions WHERE target_identity_id=$1) + (SELECT count(*) FROM integrity_signals WHERE subject_identity_id=$1)",
    )
    .bind(account.id)
    .fetch_one(&audit_pool)
    .await
    .unwrap();
    assert_eq!(removed_derived_records, 0);
    let unrelated_report_survives: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM player_reports WHERE id=$1)")
            .bind(direct_action_report_id)
            .fetch_one(&audit_pool)
            .await
            .unwrap();
    assert!(unrelated_report_survives);
    let direct_action_survives: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM player_moderation_actions WHERE id=$1)")
            .bind(direct_action_id)
            .fetch_one(&audit_pool)
            .await
            .unwrap();
    assert!(!direct_action_survives);

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
    second.hub.connect(session_id, PROTOCOL_VERSION, sender);

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
