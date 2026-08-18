use std::{
    net::SocketAddr,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use chrono::Utc;
use mk01_server::{
    AppState, MAX_SUPPORTED_PROTOCOL_VERSION, MIN_SUPPORTED_PROTOCOL_VERSION, PROTOCOL_VERSION,
    app::hash_token,
    build_router,
    config::{Settings, StorageMode},
    domain::{
        Coordinate, GameRoom, Orientation, RoomVisibility, ShipKind, ShipPlacement, UserSession,
    },
    protocol::{
        PROTOCOL_CAPABILITIES, PROTOCOL_CAPABILITIES_HEADER, PROTOCOL_MAX_VERSION_HEADER,
        PROTOCOL_MIN_VERSION_HEADER, PROTOCOL_VERSION_HEADER,
    },
    store::{GameStore, MemoryStore},
};
use serde_json::{Value, json};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error as WebSocketError, client::IntoClientRequest},
};
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

async fn upgrade_account(app: &Router, guest_cookie: &str, handle: &str) -> (String, Value) {
    let response = send(
        app,
        Request::builder()
            .method("POST")
            .uri("/api/accounts/upgrade")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, guest_cookie)
            .body(Body::from(json!({ "handle": handle }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let body = json_body(response).await;
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
    assert_eq!(created["snapshot"]["balance"]["rulesetVersion"], 1);
    assert_eq!(
        created["snapshot"]["balance"]["checksum"],
        "6e6a17885e5203e30456ec9fe2f6d663541ec6d01df153cf352bac0314aafa76"
    );
    assert_eq!(created["snapshot"]["balance"]["manifest"]["boardSize"], 10);
    assert_eq!(
        created["snapshot"]["balance"]["manifest"]["fleet"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
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
async fn completed_history_and_replay_keep_the_original_balance_interpretation() {
    let store = Arc::new(MemoryStore::default());
    let app = build_router(AppState::with_store(test_settings(), store.clone()));
    let (alpha_cookie, _) = create_session(&app, "Balance Alpha").await;
    let (bravo_cookie, _) = create_session(&app, "Balance Bravo").await;
    let alpha_token = alpha_cookie.split_once('=').unwrap().1;
    let bravo_token = bravo_cookie.split_once('=').unwrap().1;
    let alpha = store
        .session_by_token_hash(&hash_token(alpha_token))
        .await
        .unwrap()
        .unwrap();
    let bravo = store
        .session_by_token_hash(&hash_token(bravo_token))
        .await
        .unwrap()
        .unwrap();
    let mut room = GameRoom::new(
        "BAL001".to_string(),
        "Pinned operation".to_string(),
        RoomVisibility::Private,
        &alpha,
    )
    .unwrap();
    room.join(&bravo).unwrap();
    let alpha_player = room.player_for_session(alpha.id).unwrap().id;
    let bravo_player = room.player_for_session(bravo.id).unwrap().id;
    room.set_lobby_ready(alpha.id, uuid::Uuid::new_v4(), alpha_player, true)
        .unwrap();
    room.set_lobby_ready(bravo.id, uuid::Uuid::new_v4(), bravo_player, true)
        .unwrap();
    room.start_placement(alpha.id, uuid::Uuid::new_v4(), alpha_player, room.version)
        .unwrap();
    room.place_ships(alpha.id, fleet(0)).unwrap();
    room.place_ships(bravo.id, fleet(5)).unwrap();
    room.confirm_placement(alpha.id, &fleet(0), 60).unwrap();
    room.confirm_placement(bravo.id, &fleet(5), 60).unwrap();
    room.surrender(bravo.id, bravo_player).unwrap();
    let room_id = room.id;
    store.save_room(&mut room).await.unwrap();

    let history = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/games/history")
                .header(header::COOKIE, &alpha_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(history["games"][0]["balance"]["rulesetVersion"], 1);
    assert_eq!(
        history["games"][0]["balance"]["manifest"]["consecutiveTimeoutForfeit"],
        3
    );

    let replay = json_body(
        send(
            &app,
            Request::builder()
                .uri(format!("/api/games/{room_id}/replay"))
                .header(header::COOKIE, &alpha_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(replay["rulesetVersion"], 1);
    assert_eq!(replay["balance"], history["games"][0]["balance"]);
    assert_eq!(
        replay["balance"]["checksum"],
        "6e6a17885e5203e30456ec9fe2f6d663541ec6d01df153cf352bac0314aafa76"
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
    assert!(archive["rankedLeaderboardEntries"].is_array());
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
    assert!(metrics.contains("# TYPE mk01_http_responses_total counter"));
    assert!(metrics.contains("# TYPE mk01_command_duration_milliseconds histogram"));
    assert!(metrics.contains("# TYPE mk01_matchmaking_duration_seconds histogram"));
    assert!(metrics.contains("# TYPE mk01_active_match_recovery_milliseconds histogram"));
    assert!(metrics.contains("mk01_http_responses_total{class=\"2xx\"} 0"));
    assert!(metrics.contains(
        "mk01_command_duration_milliseconds_count{transport=\"http\",outcome=\"accepted\"} 0"
    ));
    assert!(metrics.contains("# TYPE mk01_protocol_negotiations_total counter"));
}

#[tokio::test]
async fn protocol_window_accepts_headerless_v2_and_rejects_unsupported_clients() {
    let state = AppState::with_store(test_settings(), Arc::new(MemoryStore::default()));
    let app = build_router(state.clone());
    let legacy_response = send(
        &app,
        Request::builder()
            .uri("/api/protocol")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(legacy_response.status(), StatusCode::OK);
    assert_eq!(
        legacy_response.headers()[PROTOCOL_VERSION_HEADER],
        PROTOCOL_VERSION.to_string()
    );
    assert_eq!(
        legacy_response.headers()[PROTOCOL_MIN_VERSION_HEADER],
        MIN_SUPPORTED_PROTOCOL_VERSION.to_string()
    );
    assert_eq!(
        legacy_response.headers()[PROTOCOL_MAX_VERSION_HEADER],
        MAX_SUPPORTED_PROTOCOL_VERSION.to_string()
    );
    assert_eq!(
        legacy_response.headers()[PROTOCOL_CAPABILITIES_HEADER],
        PROTOCOL_CAPABILITIES.join(",")
    );
    let descriptor = json_body(legacy_response).await;
    assert_eq!(descriptor["currentVersion"], PROTOCOL_VERSION);
    assert_eq!(descriptor["legacyDefaultVersion"], PROTOCOL_VERSION);
    assert_eq!(
        descriptor["capabilities"].as_array().unwrap().len(),
        PROTOCOL_CAPABILITIES.len()
    );

    let explicit_response = send(
        &app,
        Request::builder()
            .uri("/api/health")
            .header(PROTOCOL_VERSION_HEADER, PROTOCOL_VERSION.to_string())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(explicit_response.status(), StatusCode::OK);

    for unsupported in ["1", "3", "invalid"] {
        let response = send(
            &app,
            Request::builder()
                .uri("/api/health")
                .header(PROTOCOL_VERSION_HEADER, unsupported)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);
        assert_eq!(
            response.headers()[PROTOCOL_MIN_VERSION_HEADER],
            MIN_SUPPORTED_PROTOCOL_VERSION.to_string()
        );
        assert_eq!(
            json_body(response).await["code"],
            "SERVER_PROTOCOL_MISMATCH"
        );
    }
    assert_eq!(
        state.metrics.protocol_http_negotiations[0].load(Ordering::Relaxed),
        2
    );
    assert_eq!(
        state
            .metrics
            .protocol_http_rejections
            .load(Ordering::Relaxed),
        3
    );
}

#[tokio::test]
async fn websocket_handshake_supports_old_and_new_v2_clients_and_rejects_v3_only() {
    let state = AppState::with_store(test_settings(), Arc::new(MemoryStore::default()));
    let app = build_router(state.clone());
    let (cookie, _) = create_session(&app, "Protocol Captain").await;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let request = |offered_protocol: Option<&'static str>| {
        let mut request = format!("ws://{address}/ws").into_client_request().unwrap();
        request
            .headers_mut()
            .insert(header::ORIGIN, "http://localhost:5173".parse().unwrap());
        request
            .headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());
        if let Some(offered_protocol) = offered_protocol {
            request.headers_mut().insert(
                header::SEC_WEBSOCKET_PROTOCOL,
                offered_protocol.parse().unwrap(),
            );
        }
        request
    };

    let (mut legacy_socket, legacy_response) = connect_async(request(None)).await.unwrap();
    assert_eq!(legacy_response.status(), StatusCode::SWITCHING_PROTOCOLS);
    assert!(
        legacy_response
            .headers()
            .get(header::SEC_WEBSOCKET_PROTOCOL)
            .is_none()
    );
    legacy_socket.close(None).await.unwrap();

    let (mut v2_socket, v2_response) = connect_async(request(Some("mk01.v3, mk01.v2")))
        .await
        .unwrap();
    assert_eq!(v2_response.status(), StatusCode::SWITCHING_PROTOCOLS);
    assert_eq!(
        v2_response.headers()[header::SEC_WEBSOCKET_PROTOCOL],
        "mk01.v2"
    );
    v2_socket.close(None).await.unwrap();

    let error = connect_async(request(Some("mk01.v3"))).await.unwrap_err();
    let WebSocketError::Http(response) = error else {
        panic!("expected an HTTP protocol rejection, got {error}");
    };
    assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);
    assert_eq!(
        state.metrics.protocol_websocket_negotiations[0].load(Ordering::Relaxed),
        2
    );
    assert_eq!(
        state
            .metrics
            .protocol_websocket_rejections
            .load(Ordering::Relaxed),
        1
    );

    server.abort();
}

#[tokio::test]
async fn new_player_funnel_accepts_only_bounded_anonymous_dimensions() {
    let app = test_app();
    for event in [
        json!({ "stage": "landing", "outcome": "reached" }),
        json!({
            "stage": "room_joined",
            "outcome": "failed",
            "reason": "room_entry"
        }),
        json!({ "stage": "tutorial_started", "outcome": "abandoned" }),
    ] {
        let response = send(
            &app,
            Request::builder()
                .method("POST")
                .uri("/api/telemetry/funnel")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(event.to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    for invalid in [
        json!({ "stage": "landing", "outcome": "failed" }),
        json!({ "stage": "landing", "outcome": "reached", "reason": "network" }),
        json!({ "stage": "player-123", "outcome": "reached" }),
        json!({ "stage": "landing", "outcome": "reached", "playerId": "secret" }),
    ] {
        let response = send(
            &app,
            Request::builder()
                .method("POST")
                .uri("/api/telemetry/funnel")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(invalid.to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    let response = send(
        &app,
        Request::builder()
            .uri("/api/metrics")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let metrics = String::from_utf8(
        to_bytes(response.into_body(), 128 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        metrics.contains(
            "mk01_new_player_funnel_events_total{stage=\"landing\",outcome=\"reached\"} 1"
        )
    );
    assert!(metrics.contains(
        "mk01_new_player_funnel_events_total{stage=\"tutorial_started\",outcome=\"abandoned\"} 1"
    ));
    assert!(metrics.contains(
        "mk01_new_player_funnel_events_total{stage=\"room_joined\",outcome=\"failed\"} 1"
    ));
    assert!(metrics.contains("mk01_new_player_funnel_failures_total{reason=\"room_entry\"} 1"));
    assert!(!metrics.contains("playerId"));
    assert!(!metrics.contains("secret"));
}

#[tokio::test]
async fn real_user_performance_accepts_only_bounded_anonymous_histogram_samples() {
    let app = test_app();
    for sample in [
        json!({
            "metric": "lcp",
            "route": "landing",
            "deviceTier": "desktop",
            "value": 2400
        }),
        json!({
            "metric": "cls",
            "route": "landing",
            "deviceTier": "desktop",
            "value": 80
        }),
        json!({
            "metric": "inp",
            "route": "lobby",
            "deviceTier": "mobile",
            "value": 180
        }),
        json!({
            "metric": "battle_interaction",
            "route": "room",
            "deviceTier": "low_mobile",
            "value": 420
        }),
    ] {
        let response = send(
            &app,
            Request::builder()
                .method("POST")
                .uri("/api/telemetry/performance")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(sample.to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    for invalid in [
        json!({
            "metric": "lcp",
            "route": "landing",
            "deviceTier": "desktop",
            "value": 60001
        }),
        json!({
            "metric": "cls",
            "route": "landing",
            "deviceTier": "desktop",
            "value": 5001
        }),
        json!({
            "metric": "fps",
            "route": "landing",
            "deviceTier": "desktop",
            "value": 60
        }),
        json!({
            "metric": "inp",
            "route": "player-123",
            "deviceTier": "mobile",
            "value": 100
        }),
        json!({
            "metric": "inp",
            "route": "lobby",
            "deviceTier": "phone-model-123",
            "value": 100
        }),
        json!({
            "metric": "inp",
            "route": "lobby",
            "deviceTier": "mobile",
            "value": -1
        }),
        json!({
            "metric": "inp",
            "route": "lobby",
            "deviceTier": "mobile",
            "value": 1.5
        }),
        json!({
            "metric": "inp",
            "route": "lobby",
            "deviceTier": "mobile",
            "value": 100,
            "sessionId": "secret"
        }),
    ] {
        let response = send(
            &app,
            Request::builder()
                .method("POST")
                .uri("/api/telemetry/performance")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(invalid.to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    let response = send(
        &app,
        Request::builder()
            .uri("/api/metrics")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let metrics = String::from_utf8(
        to_bytes(response.into_body(), 256 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(metrics.contains(
        "mk01_rum_lcp_milliseconds_bucket{route=\"landing\",device_tier=\"desktop\",le=\"2500\"} 1"
    ));
    assert!(
        metrics.contains("mk01_rum_cls_milli_sum{route=\"landing\",device_tier=\"desktop\"} 80")
    );
    assert!(
        metrics
            .contains("mk01_rum_inp_milliseconds_count{route=\"lobby\",device_tier=\"mobile\"} 1")
    );
    assert!(metrics.contains(
        "mk01_rum_battle_interaction_milliseconds_bucket{route=\"room\",device_tier=\"low_mobile\",le=\"500\"} 1"
    ));
    assert!(!metrics.contains("sessionId"));
    assert!(!metrics.contains("secret"));
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
async fn ranked_matchmaking_requires_accounts_rejects_client_authority_and_returns_quality() {
    let app = test_app();
    let (guest_cookie, _) = create_session(&app, "Rank Guest").await;
    let ranked_body =
        || Body::from(json!({ "pool": "RANKED", "region": "KOREA", "latencyMs": 55 }).to_string());
    let shared_route_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/matchmaking")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &guest_cookie)
            .body(ranked_body())
            .unwrap(),
    )
    .await;
    assert_eq!(shared_route_response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_body(shared_route_response).await["code"],
        "INVALID_REQUEST"
    );
    let guest_response = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/matchmaking/ranked")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &guest_cookie)
            .body(ranked_body())
            .unwrap(),
    )
    .await;
    assert_eq!(guest_response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(guest_response).await["code"],
        "RANKED_ACCOUNT_REQUIRED"
    );

    let injected = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/matchmaking/ranked")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &guest_cookie)
            .body(Body::from(
                json!({
                    "pool": "RANKED",
                    "region": "KOREA",
                    "latencyMs": 55,
                    "rating": 4000,
                    "partyId": uuid::Uuid::new_v4()
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(injected.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(injected).await["code"], "INVALID_REQUEST");

    let (first_guest_cookie, _) = create_session(&app, "Rank Alpha").await;
    let (second_guest_cookie, _) = create_session(&app, "Rank Bravo").await;
    let (first_cookie, first_account) =
        upgrade_account(&app, &first_guest_cookie, "Rank Alpha").await;
    let (second_cookie, second_account) =
        upgrade_account(&app, &second_guest_cookie, "Rank Bravo").await;
    assert_ne!(
        first_account["account"]["id"],
        second_account["account"]["id"]
    );

    let enqueue = |cookie: &str| {
        Request::builder()
            .method("POST")
            .uri("/api/matchmaking/ranked")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, cookie)
            .body(ranked_body())
            .unwrap()
    };
    let first_response = send(&app, enqueue(&first_cookie)).await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_queued = json_body(first_response).await;
    assert_eq!(first_queued["queued"], true);
    assert_eq!(first_queued["ticket"]["pool"], "RANKED");
    assert_eq!(first_queued["ticket"]["region"], "KOREA");
    assert_eq!(first_queued["ticket"]["reportedLatencyMs"], 55);
    assert_eq!(first_queued["ticket"]["rating"], 1500);
    assert_eq!(first_queued["ticket"]["partySize"], 1);
    assert_eq!(first_queued["ticket"]["searchWindow"]["phase"], "EXACT");
    assert!(first_queued["ticket"].get("partyId").is_none());
    let profile = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/profile")
                .header(header::COOKIE, &first_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(profile["ranked"]["seasonId"], "FOUNDERS_SEASON");
    assert_eq!(profile["ranked"]["tier"], "PROVISIONAL");
    assert_eq!(profile["ranked"]["placementMatchesRemaining"], 5);

    let second_response = send(&app, enqueue(&second_cookie)).await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let matched = json_body(second_response).await;
    assert_eq!(matched["queued"], false);
    assert_eq!(matched["matchQuality"]["pool"], "RANKED");
    assert_eq!(matched["matchQuality"]["phase"], "EXACT");
    assert_eq!(matched["matchQuality"]["ratingDelta"], 0);
    assert_eq!(matched["matchQuality"]["recentPairings"], 0);
    assert_eq!(matched["matchQuality"]["rematchRelaxed"], false);
    assert!(matched["matchQuality"]["sharedWaitSeconds"].is_number());
    assert!(matched["matchQuality"]["waitSkewSeconds"].is_number());
    assert_eq!(
        matched["snapshot"]["matchmakingQuality"],
        matched["matchQuality"]
    );
    assert_eq!(matched["snapshot"]["room"]["name"], "랭크 교전");
    assert_eq!(
        matched["snapshot"]["rankedMatch"]["seasonId"],
        "FOUNDERS_SEASON"
    );
}

#[tokio::test]
async fn ranked_leaderboard_requires_accounts_bounds_queries_and_persists_privacy_choice() {
    let app = test_app();
    let (guest_cookie, _) = create_session(&app, "Board Guest").await;
    let guest_response = send(
        &app,
        Request::builder()
            .uri("/api/leaderboards/ranked")
            .header(header::COOKIE, &guest_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(guest_response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(guest_response).await["code"],
        "RANKED_ACCOUNT_REQUIRED"
    );

    let (account_cookie, _) = upgrade_account(&app, &guest_cookie, "Board Captain").await;
    let oversized = send(
        &app,
        Request::builder()
            .uri("/api/leaderboards/ranked?limit=51")
            .header(header::COOKIE, &account_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(oversized.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(oversized).await["code"], "INVALID_REQUEST");
    let unknown_season = send(
        &app,
        Request::builder()
            .uri("/api/leaderboards/ranked?seasonId=UNKNOWN_SEASON")
            .header(header::COOKIE, &account_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(unknown_season.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(unknown_season).await["code"], "INVALID_REQUEST");

    let leaderboard = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/leaderboards/ranked?limit=20")
                .header(header::COOKIE, &account_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(leaderboard["seasonId"], "FOUNDERS_SEASON");
    assert_eq!(leaderboard["archived"], false);
    assert_eq!(leaderboard["viewerVisible"], true);
    assert_eq!(leaderboard["entries"], json!([]));
    assert!(leaderboard["generatedAt"].is_string());
    assert_eq!(
        leaderboard["availableSeasons"][0]["seasonId"],
        "FOUNDERS_SEASON"
    );
    assert!(leaderboard.get("accountId").is_none());

    let visibility = json_body(
        send(
            &app,
            Request::builder()
                .method("PUT")
                .uri("/api/profile/leaderboard-visibility")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, &account_cookie)
                .body(Body::from(json!({ "visible": false }).to_string()))
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(visibility["visible"], false);

    let hidden = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/leaderboards/ranked")
                .header(header::COOKIE, &account_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(hidden["viewerVisible"], false);

    let exported = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/accounts/export")
                .header(header::COOKIE, &account_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(exported["leaderboardVisible"], false);
}

#[tokio::test]
async fn support_console_finds_exact_accounts_and_audits_session_revocation() {
    let app = test_app();
    let (guest_cookie, _) = create_session(&app, "Support Captain").await;
    let (account_cookie, upgraded) = upgrade_account(&app, &guest_cookie, "SupportCaptain").await;
    let account_id = upgraded["account"]["id"].as_str().unwrap();

    let unauthenticated = send(
        &app,
        Request::builder()
            .uri("/api/admin/support/accounts?query=SupportCaptain")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let lookup = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/admin/support/accounts?query=SupportCaptain")
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
    assert_eq!(lookup["account"]["id"], account_id);
    assert_eq!(lookup["account"]["handle"], "SupportCaptain");
    assert_eq!(lookup["sessions"].as_array().unwrap().len(), 1);
    assert!(lookup.get("recoveryKey").is_none());
    assert!(lookup.get("tokenHash").is_none());

    let missing_operator = send(
        &app,
        Request::builder()
            .method("POST")
            .uri(format!(
                "/api/admin/support/accounts/{account_id}/sessions/revoke"
            ))
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                header::AUTHORIZATION,
                "Bearer integration-admin-token-32-characters-long",
            )
            .body(Body::from(
                json!({ "reason": "Verified account-recovery request" }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(missing_operator.status(), StatusCode::BAD_REQUEST);

    let revoked = json_body(
        send(
            &app,
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/admin/support/accounts/{account_id}/sessions/revoke"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .header(
                    header::AUTHORIZATION,
                    "Bearer integration-admin-token-32-characters-long",
                )
                .header("x-operator-id", "player-support-test")
                .body(Body::from(
                    json!({ "reason": "Verified account-recovery request" }).to_string(),
                ))
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(revoked["action"]["action"], "REVOKE_ALL_SESSIONS");
    assert_eq!(revoked["action"]["operatorId"], "player-support-test");
    assert_eq!(
        revoked["action"]["affectedSessionIds"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let revoked_session = send(
        &app,
        Request::builder()
            .uri("/api/sessions/current")
            .header(header::COOKIE, &account_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(revoked_session.status(), StatusCode::UNAUTHORIZED);

    let audited = json_body(
        send(
            &app,
            Request::builder()
                .uri(format!("/api/admin/support/accounts?query={account_id}"))
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
    assert!(audited["sessions"].as_array().unwrap().is_empty());
    assert_eq!(audited["actions"].as_array().unwrap().len(), 1);
    assert_eq!(
        audited["actions"][0]["reason"],
        "Verified account-recovery request"
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

#[tokio::test]
async fn live_content_is_validated_versioned_applied_and_rollback_safe() {
    let app = test_app();
    let (cookie, _) = create_session(&app, "LiveOps Tester").await;
    let now = Utc::now();
    let payload = json!({
        "activateAt": now,
        "season": {
            "id": "NORTH_SEA_01",
            "title": "북해 통제 시즌",
            "description": "북해 전역의 작전 우위를 확보하고 시즌 전공을 기록하십시오.",
            "startsAt": now - chrono::Duration::days(1),
            "endsAt": now + chrono::Duration::days(30)
        },
        "events": [{
            "id": "CONVOY_GUARD",
            "title": "수송선단 호위",
            "description": "기간 임무를 완수해 북해 수송 항로를 안전하게 유지하십시오.",
            "startsAt": now - chrono::Duration::hours(1),
            "endsAt": now + chrono::Duration::days(7)
        }],
        "featureFlags": {
            "missionsEnabled": true,
            "eventBannerEnabled": true
        },
        "tuning": {
            "dailyDeploymentRewardXp": 175,
            "dailyAccuracyRewardXp": 225,
            "weeklySupremacyRewardXp": 650
        },
        "changeNote": "Launch the validated North Sea content"
    });

    let public_baseline = send(
        &app,
        Request::builder()
            .uri("/api/content/live")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(public_baseline.status(), StatusCode::OK);
    assert_eq!(json_body(public_baseline).await["revision"], 0);

    let unauthorized = send(
        &app,
        Request::builder()
            .uri("/api/admin/content/revisions")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let validate = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/admin/content/validate")
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                header::AUTHORIZATION,
                "Bearer integration-admin-token-32-characters-long",
            )
            .header("x-operator-id", "liveops-test")
            .body(Body::from(
                json!({ "expectedRevision": 0, "payload": payload }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(validate.status(), StatusCode::OK);
    let validation = json_body(validate).await;
    assert_eq!(validation["valid"], true);
    assert_eq!(validation["candidateRevision"], 1);
    assert_eq!(validation["issues"].as_array().unwrap().len(), 0);

    let publish = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/admin/content/revisions")
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                header::AUTHORIZATION,
                "Bearer integration-admin-token-32-characters-long",
            )
            .header("x-operator-id", "liveops-test")
            .body(Body::from(
                json!({ "expectedRevision": 0, "payload": payload }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(publish.status(), StatusCode::CREATED);
    let published = json_body(publish).await;
    assert_eq!(published["revision"], 1);
    assert_eq!(published["operatorId"], "liveops-test");

    let profile = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/profile")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(profile["liveContent"]["revision"], 1);
    assert_eq!(profile["liveContent"]["season"]["id"], "NORTH_SEA_01");
    assert_eq!(profile["missions"][0]["rewardXp"], 175);

    let stale_publish = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/admin/content/revisions")
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                header::AUTHORIZATION,
                "Bearer integration-admin-token-32-characters-long",
            )
            .header("x-operator-id", "stale-liveops-test")
            .body(Body::from(
                json!({ "expectedRevision": 0, "payload": payload }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(stale_publish.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(stale_publish).await["code"],
        "LIVE_CONTENT_REVISION_CONFLICT"
    );

    let mut unsafe_payload = payload.clone();
    unsafe_payload["tuning"]["weeklySupremacyRewardXp"] = json!(10_000);
    unsafe_payload["changeNote"] = json!("Attempt unsafe reward increase");
    let unsafe_validation = json_body(
        send(
            &app,
            Request::builder()
                .method("POST")
                .uri("/api/admin/content/validate")
                .header(header::CONTENT_TYPE, "application/json")
                .header(
                    header::AUTHORIZATION,
                    "Bearer integration-admin-token-32-characters-long",
                )
                .header("x-operator-id", "liveops-test")
                .body(Body::from(
                    json!({ "expectedRevision": 1, "payload": unsafe_payload }).to_string(),
                ))
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(unsafe_validation["valid"], false);
    assert!(
        unsafe_validation["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["code"] == "TUNING_OUT_OF_RANGE")
    );

    let mut disabled_payload = payload.clone();
    disabled_payload["activateAt"] = json!(Utc::now());
    disabled_payload["featureFlags"]["missionsEnabled"] = json!(false);
    disabled_payload["featureFlags"]["eventBannerEnabled"] = json!(false);
    disabled_payload["changeNote"] = json!("Exercise emergency mission kill switch");
    let disabled = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/admin/content/revisions")
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                header::AUTHORIZATION,
                "Bearer integration-admin-token-32-characters-long",
            )
            .header("x-operator-id", "incident-commander")
            .body(Body::from(
                json!({ "expectedRevision": 1, "payload": disabled_payload }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(disabled.status(), StatusCode::CREATED);
    assert_eq!(json_body(disabled).await["revision"], 2);
    let disabled_profile = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/profile")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert!(disabled_profile["missions"].as_array().unwrap().is_empty());
    assert!(
        disabled_profile["liveContent"]["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let rollback = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/admin/content/rollback")
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                header::AUTHORIZATION,
                "Bearer integration-admin-token-32-characters-long",
            )
            .header("x-operator-id", "incident-commander")
            .body(Body::from(
                json!({
                    "expectedRevision": 2,
                    "targetRevision": 0,
                    "changeNote": "Rollback to the built-in safe baseline"
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(rollback.status(), StatusCode::OK);
    let rolled_back = json_body(rollback).await;
    assert_eq!(rolled_back["revision"], 3);
    assert_eq!(rolled_back["rolledBackFromRevision"], 0);
    let restored_baseline = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/content/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(restored_baseline["revision"], 3);
    assert_eq!(restored_baseline["season"]["id"], "FOUNDERS_SEASON");
    assert_eq!(restored_baseline["featureFlags"]["missionsEnabled"], true);

    let history = json_body(
        send(
            &app,
            Request::builder()
                .uri("/api/admin/content/revisions?limit=10")
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
    assert_eq!(history["currentRevision"], 3);
    assert_eq!(
        history["revisions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|revision| revision["revision"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![3, 2, 1, 0]
    );

    let metrics = to_bytes(
        send(
            &app,
            Request::builder()
                .uri("/api/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .into_body(),
        128 * 1024,
    )
    .await
    .unwrap();
    let metrics = String::from_utf8(metrics.to_vec()).unwrap();
    assert!(metrics.contains("mk01_live_content_published_total 2"));
    assert!(metrics.contains("mk01_live_content_rollbacks_total 1"));
}
