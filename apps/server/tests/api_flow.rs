use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use mk01_server::{
    AppState, PROTOCOL_VERSION, build_router,
    config::{Settings, StorageMode},
    store::MemoryStore,
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
        trust_proxy_headers: false,
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
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        SocketAddr::from(([127, 0, 0, 1], 45_000)),
    ));
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
                json!({ "name": "North Sea", "visibility": "PUBLIC" }).to_string(),
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
