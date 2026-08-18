use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use chrono::Utc;
use mk01_server::{
    AppState, PROTOCOL_VERSION,
    app::hash_token,
    build_router,
    config::{Settings, StorageMode},
    domain::{
        Coordinate, GameRoom, Orientation, RoomVisibility, ShipKind, ShipPlacement, UserSession,
    },
    store::{GameStore, MemoryStore},
};
use serde_json::{Value, json};
use tower::ServiceExt;

fn test_settings() -> Settings {
    Settings {
        bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
        storage_mode: StorageMode::Memory,
        database_url: String::new(),
        redis_url: String::new(),
        allowed_origins: vec!["http://localhost:5173".to_string()],
        secure_cookies: false,
        session_ttl: Duration::from_secs(3_600),
        reconnect_grace: Duration::from_secs(1),
        turn_duration_seconds: 60,
        public_base_url: "http://localhost:5173".to_string(),
        api_requests_per_minute: 1_000,
        http_requests_per_minute_per_ip: 2_000,
        session_creations_per_minute: 100,
        websocket_events_per_second: 100,
        websocket_send_queue_capacity: 32,
        max_websocket_connections: 100,
        max_active_rooms: 100,
        max_matchmaking_queue: 100,
        completed_room_retention: Duration::from_secs(60 * 60 * 24 * 90),
        matchmaking_entry_ttl: Duration::from_secs(600),
        retention_sweep_interval: Duration::from_secs(3_600),
        moderation_retention: Duration::from_secs(60 * 60 * 24 * 365),
        integrity_signal_retention: Duration::from_secs(60 * 60 * 24 * 180),
        trust_proxy_headers: false,
        distributed_coordination_required: false,
        admin_token_hash: Some(hash_token("integration-admin-token-32-characters-long")),
    }
}

fn test_app_with_settings(settings: Settings) -> Router {
    build_router(AppState::with_store(
        settings,
        Arc::new(MemoryStore::default()),
    ))
}

fn test_app() -> Router {
    test_app_with_settings(test_settings())
}

async fn send(app: &Router, request: Request<Body>) -> Response<Body> {
    let mut request = request;
    request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(SocketAddr::from((
            [127, 0, 0, 1],
            45_000,
        ))));
    app.clone().oneshot(request).await.unwrap()
}

async fn json_body(response: Response<Body>) -> Value {
    let bytes = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn create_session(app: &Router, nickname: &str) -> (String, Value) {
    let response = send(
        app,
        Request::builder()
            .method("POST")
            .uri("/api/sessions")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({ "nickname": nickname }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Lax"));
    let cookie = set_cookie.split(';').next().unwrap().to_string();
    let body = json_body(response).await;
    assert!(body.get("playerToken").is_none());
    (cookie, body)
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

#[tokio::test]
async fn guest_sessions_create_join_and_recover_a_two_player_room() {
    let app = test_app();
    let (host_cookie, host_session) = create_session(&app, "Alpha").await;

    let malformed_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/sessions")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{\"nickname\":"))
            .unwrap(),
    )
    .await;
    assert_eq!(malformed_response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_body(malformed_response).await["code"],
        "INVALID_REQUEST"
    );

    let create_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/rooms")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &host_cookie)
            .body(Body::from(
                json!({
                    "name": "North Sea",
                    "visibility": "PUBLIC",
                    "rules": { "mode": "SALVO", "turnDurationSeconds": 90 }
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created = json_body(create_response).await;
    let room_id = created["snapshot"]["room"]["id"].as_str().unwrap();
    let room_code = created["snapshot"]["room"]["code"].as_str().unwrap();
    assert_eq!(
        created["snapshot"]["room"]["status"],
        "WAITING_FOR_OPPONENT"
    );
    assert_eq!(created["snapshot"]["roomState"], "WAITING_FOR_OPPONENT");
    assert_eq!(
        created["snapshot"]["hostPlayerId"],
        created["snapshot"]["selfPlayerId"]
    );
    assert_eq!(created["snapshot"]["canStartGame"], false);
    assert_eq!(created["snapshot"]["rules"]["mode"], "SALVO");
    assert_eq!(
        created["snapshot"]["room"]["rules"]["turnDurationSeconds"],
        90
    );
    assert!(created["inviteUrl"].as_str().unwrap().ends_with(room_code));

    let list_response = send(
        &app,
        Request::builder()
            .uri("/api/rooms")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let listed = json_body(list_response).await;
    assert_eq!(listed["protocolVersion"], PROTOCOL_VERSION);
    assert_eq!(listed["rooms"].as_array().unwrap().len(), 1);

    let (guest_cookie, _) = create_session(&app, "Bravo").await;
    let join_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/rooms/join")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &guest_cookie)
            .body(Body::from(json!({ "code": room_code }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(join_response.status(), StatusCode::OK);
    let joined = json_body(join_response).await;
    assert_eq!(joined["protocolVersion"], PROTOCOL_VERSION);
    assert_eq!(joined["room"]["status"], "WAITING_FOR_READY");
    assert!(joined["gameId"].is_null());
    assert_eq!(joined["canStartGame"], false);
    assert_eq!(joined["players"].as_array().unwrap().len(), 2);
    assert!(
        joined["players"]
            .as_array()
            .unwrap()
            .iter()
            .all(|player| player.get("sessionId").is_none())
    );

    let recover_response = send(
        &app,
        Request::builder()
            .uri("/api/games/recover")
            .header(header::COOKIE, &host_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let recovered = json_body(recover_response).await;
    assert_eq!(recovered["room"]["id"], room_id);
    assert_eq!(recovered["players"].as_array().unwrap().len(), 2);

    let (third_cookie, _) = create_session(&app, "Charlie").await;
    let full_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/rooms/join")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &third_cookie)
            .body(Body::from(json!({ "code": room_code }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(full_response.status(), StatusCode::CONFLICT);
    assert_eq!(json_body(full_response).await["code"], "ROOM_FULL");

    let unauthenticated = send(
        &app,
        Request::builder()
            .uri("/api/games/recover")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(host_session["nickname"], "Alpha");

    let logout_response = send(
        &app,
        Request::builder()
            .method("DELETE")
            .uri("/api/sessions/current")
            .header(header::COOKIE, &host_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(logout_response.status(), StatusCode::NO_CONTENT);
    assert!(
        logout_response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("Max-Age=0"))
    );

    let revoked_response = send(
        &app,
        Request::builder()
            .uri("/api/sessions/current")
            .header(header::COOKIE, &host_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(revoked_response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn practice_room_is_server_authoritative_and_keeps_the_ai_fleet_private() {
    let app = test_app();
    let (cookie, _) = create_session(&app, "Trainee").await;
    let response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/practice")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &cookie)
            .body(Body::from(json!({ "difficulty": "OFFICER" }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let snapshot = json_body(response).await;
    assert_eq!(snapshot["protocolVersion"], PROTOCOL_VERSION);
    assert_eq!(snapshot["roomState"], "PLACEMENT");
    assert_eq!(snapshot["room"]["status"], "PLACEMENT");
    assert_eq!(snapshot["practiceDifficulty"], "OFFICER");
    assert!(snapshot["ownBoard"].is_null());
    assert!(snapshot["targetBoard"].is_null());
    assert!(snapshot["revealedBoard"].is_null());
    assert!(snapshot["placement"].is_null());

    let players = snapshot["players"].as_array().unwrap();
    assert_eq!(players.len(), 2);
    assert_eq!(
        players
            .iter()
            .filter(|player| player["kind"] == "AI")
            .count(),
        1
    );
    assert!(
        players
            .iter()
            .all(|player| player.get("sessionId").is_none())
    );
    assert!(
        players
            .iter()
            .find(|player| player["kind"] == "AI")
            .is_some_and(|player| player["placementConfirmed"] == true)
    );
}

#[tokio::test]
async fn guest_upgrade_login_session_listing_and_remote_revocation_preserve_identity() {
    let app = test_app();
    let (guest_cookie, _) = create_session(&app, "Navigator").await;
    let upgrade_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/accounts/upgrade")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &guest_cookie)
            .body(Body::from(json!({ "handle": "Navigator" }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(upgrade_response.status(), StatusCode::OK);
    let upgraded_cookie = upgrade_response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let upgraded = json_body(upgrade_response).await;
    let account_id = upgraded["account"]["id"].as_str().unwrap();
    let recovery_key = upgraded["recoveryKey"].as_str().unwrap();
    assert_eq!(recovery_key.len(), 43);

    assert_eq!(
        send(
            &app,
            Request::builder()
                .uri("/api/sessions/current")
                .header(header::COOKIE, &guest_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );

    let current = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/sessions/current")
                .header(header::COOKIE, &upgraded_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(current["accountId"], account_id);
    assert!(current.get("recoveryKey").is_none());
    let profile = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/profile")
                .header(header::COOKIE, &upgraded_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(profile["accountId"], account_id);
    assert_eq!(profile["level"], 1);
    assert_eq!(profile["gamesPlayed"], 0);
    assert_eq!(profile["achievements"].as_array().unwrap().len(), 5);
    assert_eq!(profile["missions"].as_array().unwrap().len(), 3);

    let bad_login = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/accounts/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "accountId": account_id, "recoveryKey": "invalid" }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(bad_login.status(), StatusCode::UNAUTHORIZED);

    let login_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/accounts/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "accountId": account_id, "recoveryKey": recovery_key }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(login_response.status(), StatusCode::CREATED);
    let login_cookie = login_response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let logged_in = json_body(login_response).await;
    assert_eq!(logged_in["accountId"], account_id);

    let sessions_response = send(
        &app,
        Request::builder()
            .uri("/api/accounts/sessions")
            .header(header::COOKIE, &login_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(sessions_response.status(), StatusCode::OK);
    let sessions = json_body(sessions_response).await;
    assert_eq!(sessions["sessions"].as_array().unwrap().len(), 2);
    let old_session_id = sessions["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|device| device["id"] != sessions["currentSessionId"])
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let revoke_response = send(
        &app,
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/accounts/sessions/{old_session_id}"))
            .header(header::COOKIE, &login_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(revoke_response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        send(
            &app,
            Request::builder()
                .uri("/api/sessions/current")
                .header(header::COOKIE, &upgraded_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn account_export_excludes_credentials_and_verified_deletion_anonymizes_rooms() {
    let store = Arc::new(MemoryStore::default());
    let app = build_router(AppState::with_store(test_settings(), store.clone()));
    let (guest_cookie, _) = create_session(&app, "Privacy Captain").await;
    let upgrade_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/accounts/upgrade")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &guest_cookie)
            .body(Body::from(
                json!({ "handle": "Privacy Captain" }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(upgrade_response.status(), StatusCode::OK);
    let account_cookie = upgrade_response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let upgraded = json_body(upgrade_response).await;
    let account_id = upgraded["account"]["id"].as_str().unwrap().to_string();
    let recovery_key = upgraded["recoveryKey"].as_str().unwrap().to_string();

    let create_room_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/rooms")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &account_cookie)
            .body(Body::from(
                json!({ "name": "Privacy operation", "visibility": "PRIVATE" }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(create_room_response.status(), StatusCode::CREATED);
    let room = json_body(create_room_response).await;
    let room_id = uuid::Uuid::parse_str(room["snapshot"]["room"]["id"].as_str().unwrap()).unwrap();

    let export_response = send(
        &app,
        Request::builder()
            .uri("/api/accounts/export")
            .header(header::COOKIE, &account_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(export_response.status(), StatusCode::OK);
    let archive = json_body(export_response).await;
    assert_eq!(archive["formatVersion"], 1);
    assert_eq!(archive["account"]["id"], account_id);
    assert_eq!(archive["credentialsExcluded"], true);
    assert_eq!(archive["sessions"].as_array().unwrap().len(), 1);
    let serialized_archive = archive.to_string();
    assert!(!serialized_archive.contains(&recovery_key));
    assert!(!serialized_archive.contains("tokenHash"));
    assert!(!serialized_archive.contains("recoveryKey"));

    let deletion_request = |key: &str, confirmation: &str| {
        Request::builder()
            .method("DELETE")
            .uri("/api/accounts")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &account_cookie)
            .body(Body::from(
                json!({ "recoveryKey": key, "confirmation": confirmation }).to_string(),
            ))
            .unwrap()
    };
    let missing_confirmation = send(&app, deletion_request(&recovery_key, "delete")).await;
    assert_eq!(missing_confirmation.status(), StatusCode::BAD_REQUEST);
    let wrong_recovery = send(&app, deletion_request(&"x".repeat(43), "DELETE")).await;
    assert_eq!(wrong_recovery.status(), StatusCode::UNAUTHORIZED);

    let deletion_response = send(&app, deletion_request(&recovery_key, "DELETE")).await;
    assert_eq!(deletion_response.status(), StatusCode::OK);
    assert!(
        deletion_response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("Max-Age=0"))
    );
    let receipt = json_body(deletion_response).await;
    assert_eq!(receipt["stats"]["sessionsDeleted"], 1);
    assert_eq!(receipt["stats"]["roomsAnonymized"], 1);
    assert!(receipt["requestId"].as_str().is_some());

    assert_eq!(
        send(
            &app,
            Request::builder()
                .uri("/api/sessions/current")
                .header(header::COOKIE, &account_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let deleted_login = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/accounts/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "accountId": account_id, "recoveryKey": recovery_key }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(deleted_login.status(), StatusCode::UNAUTHORIZED);

    let anonymized = store
        .room_by_id_authoritative(room_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(anonymized.name, "Archived Operation");
    assert_eq!(anonymized.players[0].nickname, "Deleted Commander");
    assert!(
        anonymized
            .chat_messages
            .iter()
            .all(|message| !message.content.contains("Privacy Captain"))
    );
}

#[tokio::test]
async fn completed_mission_reward_is_claimed_exactly_once_through_the_api() {
    let store = Arc::new(MemoryStore::default());
    let app = build_router(AppState::with_store(test_settings(), store.clone()));
    let (guest_cookie, _) = create_session(&app, "Reward Alpha").await;
    let upgrade_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/accounts/upgrade")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, guest_cookie)
            .body(Body::from(json!({ "handle": "Reward Alpha" }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(upgrade_response.status(), StatusCode::OK);
    let account_cookie = upgrade_response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let token = account_cookie.split_once('=').unwrap().1;
    let account_session = store
        .session_by_token_hash(&hash_token(token))
        .await
        .unwrap()
        .unwrap();
    let now = Utc::now();
    let opponent = UserSession {
        id: uuid::Uuid::new_v4(),
        account_id: None,
        nickname: "Reward Bravo".to_string(),
        token_hash: uuid::Uuid::new_v4().simple().to_string(),
        created_at: now,
        last_seen_at: now,
        current_room_id: None,
    };
    store.save_session(&opponent).await.unwrap();
    let mut room = GameRoom::new(
        "RWD234".to_string(),
        "Reward operation".to_string(),
        RoomVisibility::Private,
        &account_session,
    )
    .unwrap();
    room.join(&opponent).unwrap();
    let account_player_id = room.player_for_session(account_session.id).unwrap().id;
    let opponent_player_id = room.player_for_session(opponent.id).unwrap().id;
    room.set_lobby_ready(
        account_session.id,
        uuid::Uuid::new_v4(),
        account_player_id,
        true,
    )
    .unwrap();
    room.set_lobby_ready(opponent.id, uuid::Uuid::new_v4(), opponent_player_id, true)
        .unwrap();
    room.start_placement(
        account_session.id,
        uuid::Uuid::new_v4(),
        account_player_id,
        room.version,
    )
    .unwrap();
    room.place_ships(account_session.id, fleet(0)).unwrap();
    room.place_ships(opponent.id, fleet(5)).unwrap();
    room.confirm_placement(account_session.id, &fleet(0), 60)
        .unwrap();
    room.confirm_placement(opponent.id, &fleet(5), 60).unwrap();
    room.surrender(opponent.id, opponent_player_id).unwrap();
    store.save_room(&mut room).await.unwrap();

    let before = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/profile")
                .header(header::COOKIE, &account_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(before["gamesPlayed"], 1);
    assert_eq!(before["missions"][0]["claimable"], true);
    let xp_before = before["totalXp"].as_u64().unwrap();

    let claim = || {
        Request::builder()
            .method("POST")
            .uri("/api/profile/missions/DAILY_DEPLOYMENT/claim")
            .header(header::COOKIE, &account_cookie)
            .body(Body::empty())
            .unwrap()
    };
    let claimed = json_body(send(&app, claim()).await).await;
    assert_eq!(claimed["totalXp"], xp_before + 100);
    assert_eq!(claimed["missions"][0]["claimed"], true);
    assert_eq!(claimed["missions"][0]["claimable"], false);
    let repeated = json_body(send(&app, claim()).await).await;
    assert_eq!(repeated["totalXp"], claimed["totalXp"]);
}

#[tokio::test]
async fn production_session_cookie_has_secure_expiring_scope() {
    let app = test_app_with_settings(Settings {
        secure_cookies: true,
        ..test_settings()
    });
    let response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/sessions")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({ "nickname": "Secure" }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("Secure"));
    assert!(cookie.contains("SameSite=Lax"));
    assert!(cookie.contains("Path=/"));
    assert!(cookie.contains("Max-Age=3600"));
}

#[tokio::test]
async fn liveness_readiness_and_security_headers_are_exposed() {
    let app = test_app();
    for (path, expected_status) in [("/api/health", "ok"), ("/api/ready", "ready")] {
        let response = send(
            &app,
            Request::builder().uri(path).body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        assert_eq!(response.headers()[header::X_FRAME_OPTIONS], "DENY");
        assert!(response.headers().contains_key("content-security-policy"));
        assert_eq!(json_body(response).await["status"], expected_status);
    }

    let metrics_response = send(
        &app,
        Request::builder()
            .uri("/api/metrics")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(metrics_response.status(), StatusCode::OK);
    assert!(
        metrics_response.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/plain")
    );
    let metrics = String::from_utf8(
        to_bytes(metrics_response.into_body(), 128 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(metrics.contains("# TYPE mk01_http_requests_total counter"));
    assert!(metrics.contains("# TYPE mk01_websocket_connections gauge"));
    assert!(metrics.contains("# TYPE mk01_matchmaking_queue_depth gauge"));
    assert!(metrics.contains("# TYPE mk01_matchmaking_oldest_age_seconds gauge"));
}

#[tokio::test]
async fn global_and_session_creation_limits_reject_excess_requests() {
    let global_app = test_app_with_settings(Settings {
        http_requests_per_minute_per_ip: 1,
        ..test_settings()
    });
    let first = send(
        &global_app,
        Request::builder()
            .uri("/api/health")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let limited = send(
        &global_app,
        Request::builder()
            .uri("/api/health")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);

    let session_app = test_app_with_settings(Settings {
        session_creations_per_minute: 1,
        ..test_settings()
    });
    let _ = create_session(&session_app, "Alpha").await;
    let response = send(
        &session_app,
        Request::builder()
            .method("POST")
            .uri("/api/sessions")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({ "nickname": "Bravo" }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(json_body(response).await["code"], "RATE_LIMITED");

    let authenticated_app = test_app_with_settings(Settings {
        api_requests_per_minute: 1,
        ..test_settings()
    });
    let (cookie, _) = create_session(&authenticated_app, "Charlie").await;
    let current = || {
        Request::builder()
            .uri("/api/sessions/current")
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        send(&authenticated_app, current()).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        send(&authenticated_app, current()).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn room_and_matchmaking_capacity_limits_fail_closed() {
    let room_app = test_app_with_settings(Settings {
        max_active_rooms: 1,
        ..test_settings()
    });
    let (first_cookie, _) = create_session(&room_app, "Room Alpha").await;
    let (second_cookie, _) = create_session(&room_app, "Room Bravo").await;
    let create_room = |cookie: &str, name: &str| {
        Request::builder()
            .method("POST")
            .uri("/api/rooms")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, cookie)
            .body(Body::from(
                json!({ "name": name, "visibility": "PRIVATE" }).to_string(),
            ))
            .unwrap()
    };
    assert_eq!(
        send(&room_app, create_room(&first_cookie, "First room"))
            .await
            .status(),
        StatusCode::CREATED
    );
    let room_capacity = send(&room_app, create_room(&second_cookie, "Capacity room")).await;
    assert_eq!(room_capacity.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json_body(room_capacity).await["code"], "CAPACITY_REACHED");

    let queue_app = test_app_with_settings(Settings {
        max_matchmaking_queue: 1,
        ..test_settings()
    });
    let (queued_cookie, _) = create_session(&queue_app, "Queue Alpha").await;
    let (limited_cookie, _) = create_session(&queue_app, "Queue Bravo").await;
    let enqueue = |cookie: &str| {
        Request::builder()
            .method("POST")
            .uri("/api/matchmaking")
            .header(header::COOKIE, cookie)
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        send(&queue_app, enqueue(&queued_cookie)).await.status(),
        StatusCode::OK
    );
    let queue_capacity = send(&queue_app, enqueue(&limited_cookie)).await;
    assert_eq!(queue_capacity.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json_body(queue_capacity).await["code"], "CAPACITY_REACHED");
    assert_eq!(
        send(&queue_app, enqueue(&queued_cookie)).await.status(),
        StatusCode::OK,
        "an idempotent poll by an already queued player must remain available"
    );
}

#[tokio::test]
async fn social_safety_mutes_blocks_reports_and_prevents_future_room_pairing() {
    let app = test_app();
    let (alpha_cookie, _) = create_session(&app, "Safety Alpha").await;
    let (bravo_cookie, _) = create_session(&app, "Safety Bravo").await;

    let created_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/rooms")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &alpha_cookie)
            .body(Body::from(
                json!({ "name": "Safety room", "visibility": "PRIVATE" }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(created_response.status(), StatusCode::CREATED);
    let created = json_body(created_response).await;
    let room_id = created["snapshot"]["room"]["id"].as_str().unwrap();
    let room_code = created["snapshot"]["room"]["code"].as_str().unwrap();
    let alpha_player_id = created["snapshot"]["selfPlayerId"].as_str().unwrap();

    let joined_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/rooms/join")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &bravo_cookie)
            .body(Body::from(json!({ "code": room_code }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(joined_response.status(), StatusCode::OK);
    let joined = json_body(joined_response).await;
    let bravo_player_id = joined["selfPlayerId"].as_str().unwrap();

    let relationship_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/social/relationships")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &alpha_cookie)
            .body(Body::from(
                json!({
                    "roomId": room_id,
                    "targetPlayerId": bravo_player_id,
                    "muted": true,
                    "blocked": true
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(relationship_response.status(), StatusCode::OK);
    let relationship = json_body(relationship_response).await;
    assert_eq!(relationship["targetNickname"], "Safety Bravo");
    assert_eq!(relationship["muted"], true);
    assert_eq!(relationship["blocked"], true);

    let listed = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/social/relationships")
                .header(header::COOKIE, &alpha_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(listed["relationships"].as_array().unwrap().len(), 1);
    assert_eq!(listed["relationships"][0]["blocked"], true);

    let report_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/reports")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &alpha_cookie)
            .body(Body::from(
                json!({
                    "roomId": room_id,
                    "targetPlayerId": bravo_player_id,
                    "category": "CHAT",
                    "details": "반복적인 모욕성 채팅을 보냈습니다."
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(report_response.status(), StatusCode::CREATED);
    let report = json_body(report_response).await;
    assert_eq!(report["report"]["status"], "OPEN");
    assert!(report["report"]["reportId"].as_str().is_some());
    let report_id = report["report"]["reportId"].as_str().unwrap().to_string();

    let unauthenticated_queue = send(
        &app,
        Request::builder()
            .uri("/api/admin/moderation/reports")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(unauthenticated_queue.status(), StatusCode::UNAUTHORIZED);

    let moderation_queue = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/admin/moderation/reports?status=OPEN&search=Bravo&limit=10")
                .header(
                    header::AUTHORIZATION,
                    "Bearer integration-admin-token-32-characters-long",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(moderation_queue["cases"].as_array().unwrap().len(), 1);
    assert_eq!(moderation_queue["cases"][0]["report"]["id"], report_id);
    assert_eq!(
        moderation_queue["cases"][0]["report"]["evidence"]["roomId"],
        room_id
    );

    let action_request = |action: Value| {
        Request::builder()
            .method("POST")
            .uri(format!("/api/admin/moderation/reports/{report_id}/actions"))
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                header::AUTHORIZATION,
                "Bearer integration-admin-token-32-characters-long",
            )
            .header("x-operator-id", "trust-safety-test")
            .body(Body::from(action.to_string()))
            .unwrap()
    };
    let suspended = json_body(
        send(
            &app,
            action_request(json!({
                "action": "SUSPEND",
                "reason": "반복 위반 조사 중 임시 이용 제한",
                "durationHours": 24
            })),
        )
        .await,
    )
    .await;
    assert_eq!(suspended["action"]["action"], "SUSPEND");
    assert!(suspended["action"]["expiresAt"].as_str().is_some());
    let suspension_id = suspended["action"]["id"].as_str().unwrap();

    let suspended_session = send(
        &app,
        Request::builder()
            .uri("/api/sessions/current")
            .header(header::COOKIE, &bravo_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(suspended_session.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(suspended_session).await["code"],
        "ACCOUNT_SUSPENDED"
    );

    let reversed = json_body(
        send(
            &app,
            action_request(json!({
                "action": "REVERSE",
                "reason": "추가 검토 결과 임시 제한 해제",
                "reversesActionId": suspension_id
            })),
        )
        .await,
    )
    .await;
    assert_eq!(reversed["action"]["action"], "REVERSE");
    assert_eq!(
        send(
            &app,
            Request::builder()
                .uri("/api/sessions/current")
                .header(header::COOKIE, &bravo_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .status(),
        StatusCode::OK
    );

    let warned = json_body(
        send(
            &app,
            action_request(json!({
                "action": "WARN",
                "reason": "커뮤니티 정책 위반 경고"
            })),
        )
        .await,
    )
    .await;
    assert_eq!(warned["action"]["action"], "WARN");

    let audited_queue = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/admin/moderation/reports?status=ACTIONED")
                .header(
                    header::AUTHORIZATION,
                    "Bearer integration-admin-token-32-characters-long",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(
        audited_queue["cases"][0]["actions"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        audited_queue["cases"][0]["actions"][0]["operatorId"],
        "trust-safety-test"
    );

    let self_report = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/reports")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &alpha_cookie)
            .body(Body::from(
                json!({
                    "roomId": room_id,
                    "targetPlayerId": alpha_player_id,
                    "category": "OTHER",
                    "details": "자기 자신은 신고할 수 없습니다."
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(self_report.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(self_report).await["code"], "INVALID_REQUEST");

    for cookie in [&bravo_cookie, &alpha_cookie] {
        assert_eq!(
            send(
                &app,
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/rooms/{room_id}/leave"))
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
    }

    let bravo_room_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/rooms")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &bravo_cookie)
            .body(Body::from(
                json!({ "name": "Blocked pairing", "visibility": "PRIVATE" }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(bravo_room_response.status(), StatusCode::CREATED);
    let bravo_room = json_body(bravo_room_response).await;
    let bravo_room_code = bravo_room["snapshot"]["room"]["code"].as_str().unwrap();

    let blocked_join = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/rooms/join")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &alpha_cookie)
            .body(Body::from(json!({ "code": bravo_room_code }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(blocked_join.status(), StatusCode::FORBIDDEN);
    assert_eq!(json_body(blocked_join).await["code"], "PLAYER_BLOCKED");
}

#[tokio::test]
async fn admin_integrity_queue_is_private_filterable_and_preserves_detection_evidence() {
    use mk01_server::domain::{IntegritySignalKind, NewIntegritySignal};

    let store = Arc::new(MemoryStore::default());
    let subject_identity_id = uuid::Uuid::new_v4();
    store
        .record_integrity_signal(&NewIntegritySignal {
            id: uuid::Uuid::new_v4(),
            subject_identity_id,
            room_id: Some(uuid::Uuid::new_v4()),
            kind: IntegritySignalKind::Automation,
            severity: 3,
            confidence: 0.88,
            evidence: json!({
                "detector": "WEBSOCKET_EVENT_BURST",
                "eventsPerSecondLimit": 60
            }),
            observed_at: Utc::now(),
        })
        .await
        .unwrap();
    let app = build_router(AppState::with_store(test_settings(), store));

    let unauthorized = send(
        &app,
        Request::builder()
            .uri("/api/admin/integrity/signals")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let response = send(
        &app,
        Request::builder()
            .uri("/api/admin/integrity/signals?kind=AUTOMATION&search=event_burst")
            .header(
                header::AUTHORIZATION,
                "Bearer integration-admin-token-32-characters-long",
            )
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["signals"].as_array().unwrap().len(), 1);
    assert_eq!(body["signals"][0]["kind"], "AUTOMATION");
    assert_eq!(
        body["signals"][0]["subjectIdentityId"],
        subject_identity_id.to_string()
    );
    assert_eq!(body["signals"][0]["evidence"]["eventsPerSecondLimit"], 60);
}
