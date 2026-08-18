use super::*;
use super::{accounts::build_progression, rooms::practice_fleet, timers::select_ai_coordinate};
use crate::domain::{Coordinate, Orientation, RoomStatus, ShipKind, ShipPlacement};

fn session(nickname: &str) -> UserSession {
    let now = Utc::now();
    UserSession {
        id: Uuid::new_v4(),
        account_id: None,
        nickname: nickname.to_string(),
        token_hash: Uuid::new_v4().to_string(),
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

#[test]
fn slo_metrics_render_bounded_prometheus_series() {
    let metrics = ServerMetrics::default();
    metrics.record_http_response(StatusCode::OK);
    metrics.record_http_response(StatusCode::SERVICE_UNAVAILABLE);
    metrics.record_command_latency(CommandTransport::Http, true, Duration::from_millis(90));
    metrics.record_command_latency(
        CommandTransport::Websocket,
        false,
        Duration::from_millis(900),
    );
    metrics.record_matchmaking_latency(Utc::now() - chrono::Duration::seconds(31));
    metrics.record_active_match_recovery(Duration::from_millis(4_500));
    metrics.record_websocket_disconnect(Duration::from_millis(3_600), true);
    metrics
        .ranked_matchmaking_queued
        .store(2, Ordering::Relaxed);
    metrics
        .ranked_matchmaking_completed
        .store(1, Ordering::Relaxed);
    metrics
        .ranked_matchmaking_rematches
        .store(1, Ordering::Relaxed);
    metrics
        .ranked_leaderboard_requests
        .store(3, Ordering::Relaxed);
    metrics
        .ranked_leaderboard_empty_responses
        .store(1, Ordering::Relaxed);
    metrics
        .ranked_leaderboard_entries_served
        .store(20, Ordering::Relaxed);
    metrics
        .ranked_leaderboard_visibility_changes
        .store(2, Ordering::Relaxed);

    let output = metrics.render_prometheus(MatchmakingQueueStats {
        queued: 3,
        ranked_queued: 2,
        oldest_age_seconds: 31,
    });
    assert!(output.contains("mk01_http_responses_total{class=\"2xx\"} 1"));
    assert!(output.contains("mk01_http_responses_total{class=\"5xx\"} 1"));
    assert!(output.contains(
        "mk01_command_duration_milliseconds_bucket{transport=\"http\",outcome=\"accepted\",le=\"100\"} 1"
    ));
    assert!(output.contains(
        "mk01_command_duration_milliseconds_count{transport=\"websocket\",outcome=\"rejected\"} 1"
    ));
    assert!(output.contains("mk01_matchmaking_duration_seconds_count 1"));
    assert!(output.contains("mk01_ranked_matchmaking_queued_total 2"));
    assert!(output.contains("mk01_ranked_matchmaking_completed_total 1"));
    assert!(output.contains("mk01_ranked_matchmaking_rematches_total 1"));
    assert!(output.contains("mk01_ranked_leaderboard_requests_total 3"));
    assert!(output.contains("mk01_ranked_leaderboard_empty_responses_total 1"));
    assert!(output.contains("mk01_ranked_leaderboard_entries_served_total 20"));
    assert!(output.contains("mk01_ranked_leaderboard_visibility_changes_total 2"));
    assert!(output.contains("mk01_ranked_matchmaking_queue_depth 2"));
    assert!(output.contains("mk01_active_match_recovery_milliseconds_count 1"));
    assert!(output.contains("mk01_unexpected_disconnects_total 1"));
    assert!(output.contains("mk01_websocket_disconnects_total 1"));
    assert!(output.contains("mk01_websocket_connected_milliseconds_total 3600"));
    assert_eq!(
        output
            .matches("# TYPE mk01_command_duration_milliseconds histogram")
            .count(),
        1
    );
    assert!(!output.contains("_sum{}"));
    assert!(!output.contains("_count{}"));
}

#[test]
fn command_slo_excludes_public_and_operational_http_routes() {
    for path in [
        "/api/health",
        "/api/ready",
        "/api/metrics",
        "/api/telemetry/funnel",
        "/api/telemetry/performance",
        "/api/sessions",
        "/api/accounts/login",
        "/ws",
    ] {
        assert!(
            !router::is_product_http_command(path),
            "unexpected command: {path}"
        );
    }
    for path in [
        "/api/rooms",
        "/api/matchmaking",
        "/api/accounts/export",
        "/api/games/recover",
    ] {
        assert!(
            router::is_product_http_command(path),
            "missing command: {path}"
        );
    }
}

#[test]
fn availability_slo_excludes_probes_metrics_and_anonymous_telemetry() {
    for path in [
        "/api/health",
        "/api/ready",
        "/api/metrics",
        "/api/telemetry/funnel",
        "/api/telemetry/performance",
        "/ws",
    ] {
        assert!(
            !router::is_product_api_route(path),
            "unexpected route: {path}"
        );
    }
    for path in [
        "/api/sessions",
        "/api/accounts/login",
        "/api/rooms",
        "/api/matchmaking",
    ] {
        assert!(router::is_product_api_route(path), "missing route: {path}");
    }
}

#[tokio::test]
async fn completed_matchmaking_records_each_players_wait() {
    let store = Arc::new(MemoryStore::default());
    let alpha = session("Metric Alpha");
    let bravo = session("Metric Bravo");
    store.save_session(&alpha).await.unwrap();
    store.save_session(&bravo).await.unwrap();
    let state = AppState::with_store(Settings::default(), store);

    assert!(
        state
            .enqueue_matchmaking(alpha, MatchmakingPreferences::default())
            .await
            .unwrap()
            .room
            .is_none()
    );
    assert!(
        state
            .enqueue_matchmaking(bravo, MatchmakingPreferences::default())
            .await
            .unwrap()
            .room
            .is_some()
    );

    let output = state
        .metrics
        .render_prometheus(MatchmakingQueueStats::default());
    assert!(output.contains("mk01_matchmaking_duration_seconds_count 2"));
    assert!(output.contains("mk01_matchmaking_completed_total 1"));
}

#[tokio::test]
async fn active_match_reconnect_records_outage_to_authoritative_save() {
    let alpha = session("Recovery Alpha");
    let bravo = session("Recovery Bravo");
    let mut room = GameRoom::new(
        "SLO234".to_string(),
        "Recovery metric".to_string(),
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
    room.disconnect(alpha.id, 10).unwrap();

    let room_id = room.id;
    let store = Arc::new(MemoryStore::default());
    store.save_room(&mut room).await.unwrap();
    let settings = Settings {
        reconnect_grace: Duration::from_secs(10),
        ..Settings::default()
    };
    let state = AppState::with_store(settings, store);
    state
        .rooms
        .insert(room_id, Arc::new(Mutex::new(room.clone())));
    let reconnecting_alpha = UserSession {
        current_room_id: Some(room_id),
        ..alpha
    };

    state.restore_connection(&reconnecting_alpha).await;

    let output = state
        .metrics
        .render_prometheus(MatchmakingQueueStats::default());
    assert!(output.contains("mk01_active_match_recovery_milliseconds_count 1"));
    assert!(output.contains("mk01_active_match_recovery_milliseconds_bucket{le=\"1000\"} 1"));
    assert!(
        !state
            .room(room_id)
            .await
            .unwrap()
            .lock()
            .await
            .disconnected_deadlines
            .contains_key(&alpha_player_id)
    );
}

#[test]
fn progression_is_a_deterministic_projection_of_results() {
    use crate::domain::{FinishReason, GameResult, PlayerStatistics, WinType};

    let commander = session("Commander");
    let player_id = Uuid::new_v4();
    let now = Utc::now();
    let history = vec![GameHistoryItem {
        room_id: Uuid::new_v4(),
        room_name: "Progression test".to_string(),
        self_player_id: player_id,
        balance: crate::domain::BalancePin::current(),
        result: GameResult {
            winner_id: player_id,
            loser_id: Uuid::new_v4(),
            total_turns: 20,
            duration_seconds: 300,
            finished_at: now,
            players: vec![PlayerStatistics {
                player_id,
                shots: 20,
                hits: 12,
                ships_sunk: 5,
                accuracy: 0.6,
                total_timeouts: 0,
            }],
            finish_reason: FinishReason::FleetDestroyed,
            win_type: WinType::NormalVictory,
        },
    }];
    let live_content = baseline_live_content();
    let first = build_progression(&commander, &history, &[], None, &live_content, now);
    let repeated = build_progression(&commander, &history, &[], None, &live_content, now);
    assert_eq!(first.total_xp, 311);
    assert_eq!(first.total_xp, repeated.total_xp);
    assert_eq!(first.games_played, 1);
    assert_eq!(first.wins, 1);
    assert!(first.achievements[0].unlocked);
    assert!(first.achievements[1].unlocked);
    assert!(first.achievements[3].unlocked);
    assert!(first.missions[0].completed);
    assert!(first.missions[1].completed);
}

#[test]
fn ai_targeting_is_deterministic_and_never_repeats_a_resolved_cell() {
    let human = session("Trainee");
    let ai = session("MK-AI OFFICER");
    let mut room = GameRoom::new(
        "AI2345".to_string(),
        "AI training".to_string(),
        RoomVisibility::Private,
        &human,
    )
    .unwrap();
    room.join(&ai).unwrap();
    room.configure_practice(human.id, ai.id, AiDifficulty::Officer, practice_fleet())
        .unwrap();
    room.place_ships(human.id, fleet(0)).unwrap();
    room.confirm_placement(human.id, &fleet(0), 60).unwrap();

    let ai_player = room.player_for_session(ai.id).unwrap().clone();
    room.game.as_mut().unwrap().current_player_id = ai_player.id;
    let first = select_ai_coordinate(&room, ai_player.id).unwrap();
    assert_eq!(select_ai_coordinate(&room, ai_player.id), Some(first));
    let version = room.version;
    let turn = room.game.as_ref().unwrap().turn_number;
    room.fire(ai.id, Uuid::new_v4(), ai_player.id, first, version, turn)
        .unwrap();
    assert_ne!(select_ai_coordinate(&room, ai_player.id), Some(first));
}

#[tokio::test]
async fn restart_reclaims_an_already_expired_persisted_turn_once() {
    let first = session("Alpha");
    let second = session("Bravo");
    let mut room = GameRoom::new(
        "RST234".to_string(),
        "Restart recovery".to_string(),
        RoomVisibility::Private,
        &first,
    )
    .unwrap();
    room.join(&second).unwrap();
    let first_player_id = room.player_for_session(first.id).unwrap().id;
    let second_player_id = room.player_for_session(second.id).unwrap().id;
    room.set_lobby_ready(first.id, Uuid::new_v4(), first_player_id, true)
        .unwrap();
    room.set_lobby_ready(second.id, Uuid::new_v4(), second_player_id, true)
        .unwrap();
    room.start_placement(first.id, Uuid::new_v4(), first_player_id, room.version)
        .unwrap();
    room.place_ships(first.id, fleet(0)).unwrap();
    room.place_ships(second.id, fleet(5)).unwrap();
    room.confirm_placement(first.id, &fleet(0), 1).unwrap();
    room.confirm_placement(second.id, &fleet(5), 1).unwrap();
    let original_turn = room.game.as_ref().unwrap().turn_number;
    room.game.as_mut().unwrap().turn_deadline_at = Some(Utc::now() - chrono::Duration::seconds(1));

    let store = Arc::new(MemoryStore::default());
    store.save_room(&mut room).await.unwrap();
    let settings = Settings {
        storage_mode: StorageMode::Memory,
        turn_duration_seconds: 1,
        ..Settings::default()
    };
    let state = AppState::with_store(settings, store.clone());
    state.restore_active_rooms().await.unwrap();

    for _ in 0..40 {
        let recovered = store.room_by_id(room.id).await.unwrap().unwrap();
        if recovered.game.as_ref().unwrap().turn_number > original_turn {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let recovered = store.room_by_id(room.id).await.unwrap().unwrap();
    assert_eq!(recovered.status, RoomStatus::Playing);
    assert_eq!(
        recovered.game.as_ref().unwrap().turn_number,
        original_turn + 1
    );
    assert_eq!(
        recovered
            .game
            .as_ref()
            .unwrap()
            .total_timeout_counts
            .values()
            .sum::<u32>(),
        1
    );
}

#[tokio::test]
async fn turn_deadline_advances_without_a_client_event() {
    let first = session("Alpha");
    let second = session("Bravo");
    let mut room = GameRoom::new(
        "TMR234".to_string(),
        "Timer progression".to_string(),
        RoomVisibility::Private,
        &first,
    )
    .unwrap();
    room.join(&second).unwrap();
    let first_player_id = room.player_for_session(first.id).unwrap().id;
    let second_player_id = room.player_for_session(second.id).unwrap().id;
    room.set_lobby_ready(first.id, Uuid::new_v4(), first_player_id, true)
        .unwrap();
    room.set_lobby_ready(second.id, Uuid::new_v4(), second_player_id, true)
        .unwrap();
    room.start_placement(first.id, Uuid::new_v4(), first_player_id, room.version)
        .unwrap();
    room.place_ships(first.id, fleet(0)).unwrap();
    room.place_ships(second.id, fleet(5)).unwrap();
    room.confirm_placement(first.id, &fleet(0), 1).unwrap();
    room.confirm_placement(second.id, &fleet(5), 1).unwrap();

    let original_turn = room.game.as_ref().unwrap().turn_number;
    let original_player = room.game.as_ref().unwrap().current_player_id;
    let store = Arc::new(MemoryStore::default());
    store.save_room(&mut room).await.unwrap();
    let settings = Settings {
        storage_mode: StorageMode::Memory,
        turn_duration_seconds: 1,
        ..Settings::default()
    };
    let state = AppState::with_store(settings, store);
    state
        .rooms
        .insert(room.id, Arc::new(Mutex::new(room.clone())));
    state.schedule_turn_expiry(room.timer_state(Utc::now()));

    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let active_room = state.room(room.id).await.unwrap();
        let active_game = active_room.lock().await.game.clone().unwrap();
        if active_game.turn_number > original_turn {
            assert_ne!(active_game.current_player_id, original_player);
            assert_eq!(active_game.total_timeout_counts[&original_player], 1);
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "turn did not expire on the server"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn websocket_capacity_and_slow_consumers_are_bounded() {
    let settings = Settings {
        max_websocket_connections: 1,
        ..Settings::default()
    };
    let state = AppState::with_store(settings, Arc::new(MemoryStore::default()));
    let permit = state.try_acquire_websocket_slot().unwrap();
    assert_eq!(
        state.try_acquire_websocket_slot().unwrap_err(),
        GameError::RateLimited
    );
    drop(permit);
    assert!(state.try_acquire_websocket_slot().is_ok());

    let hub = ConnectionHub::default();
    let session_id = Uuid::new_v4();
    let (sender, _receiver) = mpsc::channel(1);
    hub.connect(session_id, crate::PROTOCOL_VERSION, sender);
    assert_eq!(
        hub.protocol_version(session_id),
        Some(crate::PROTOCOL_VERSION)
    );
    let heartbeat = || {
        ServerEvent::Heartbeat(crate::protocol::HeartbeatResponse {
            server_time: Utc::now(),
        })
    };
    hub.send(session_id, heartbeat());
    assert_eq!(hub.len(), 1);
    hub.send(session_id, heartbeat());
    assert!(hub.is_empty());
}

#[tokio::test]
async fn finished_match_assessment_detects_repeated_short_pairing_and_stalling() {
    let first = session("Integrity Alpha");
    let second = session("Integrity Bravo");
    let store = Arc::new(MemoryStore::default());
    store.save_session(&first).await.unwrap();
    store.save_session(&second).await.unwrap();
    let state = AppState::with_store(Settings::default(), store.clone());

    for index in 0..3 {
        let mut room = GameRoom::new(
            format!("INT{index}23"),
            format!("Integrity match {index}"),
            RoomVisibility::Private,
            &first,
        )
        .unwrap();
        room.join(&second).unwrap();
        let first_player_id = room.player_for_session(first.id).unwrap().id;
        let second_player_id = room.player_for_session(second.id).unwrap().id;
        room.set_lobby_ready(first.id, Uuid::new_v4(), first_player_id, true)
            .unwrap();
        room.set_lobby_ready(second.id, Uuid::new_v4(), second_player_id, true)
            .unwrap();
        room.start_placement(first.id, Uuid::new_v4(), first_player_id, room.version)
            .unwrap();
        room.place_ships(first.id, fleet(0)).unwrap();
        room.place_ships(second.id, fleet(5)).unwrap();
        room.confirm_placement(first.id, &fleet(0), 60).unwrap();
        room.confirm_placement(second.id, &fleet(5), 60).unwrap();
        if index == 2 {
            room.game
                .as_mut()
                .unwrap()
                .total_timeout_counts
                .insert(first_player_id, 3);
        }
        room.surrender(second.id, second_player_id).unwrap();
        state.save_room(&mut room).await.unwrap();
    }

    let collusion = store
        .integrity_signals(None, Some(IntegritySignalKind::Collusion), None, 25)
        .await
        .unwrap();
    assert_eq!(collusion.signals.len(), 2);
    assert!(
        collusion
            .signals
            .iter()
            .all(|signal| signal.evidence["suspiciousShortMatchesSevenDays"] == 3)
    );
    let stalling = store
        .integrity_signals(
            None,
            Some(IntegritySignalKind::IntentionalStalling),
            None,
            25,
        )
        .await
        .unwrap();
    assert_eq!(stalling.signals.len(), 1);
    assert_eq!(stalling.signals[0].evidence["totalTimeouts"], 3);
}
