use axum::{
    Json, Router,
    extract::{
        ConnectInfo, Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::IntoResponse,
    routing::{get, post},
};
use std::net::SocketAddr;
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use chrono::{Duration as ChronoDuration, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    app::{AppState, SnapshotEvent},
    domain::GameSnapshot,
    error::GameError,
    protocol::{
        CreateRoomInput, CreateSessionInput, HealthResponse, JoinRoomInput, MatchmakingResponse,
        RoomCreatedResponse, RoomListResponse, SessionResponse,
    },
    store::GameHistoryItem,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(readiness))
        .route("/sessions", post(create_session))
        .route(
            "/sessions/current",
            get(current_session).delete(delete_current_session),
        )
        .route("/rooms", get(list_rooms).post(create_room))
        .route("/rooms/join", post(join_room))
        .route("/rooms/{room_id}", get(room_state))
        .route("/rooms/{room_id}/leave", post(leave_room))
        .route("/games/recover", get(recover_game))
        .route("/games/history", get(game_history))
        .route(
            "/matchmaking",
            post(enqueue_matchmaking).delete(cancel_matchmaking),
        )
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        storage: state.store.kind(),
        server_time: Utc::now(),
        protocol_version: crate::PROTOCOL_VERSION,
    })
}

async fn readiness(State(state): State<AppState>) -> Result<Json<HealthResponse>, GameError> {
    state.store.health_check().await?;
    Ok(Json(HealthResponse {
        status: "ready",
        storage: state.store.kind(),
        server_time: Utc::now(),
        protocol_version: crate::PROTOCOL_VERSION,
    }))
}

async fn create_session(
    State(state): State<AppState>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    jar: CookieJar,
    headers: HeaderMap,
    input: Result<Json<CreateSessionInput>, JsonRejection>,
) -> Result<impl IntoResponse, GameError> {
    let client_key = client_rate_limit_key(
        &headers,
        connect_info.map(|ConnectInfo(address)| address),
        state.settings.trust_proxy_headers,
    );
    state.enforce_session_creation_rate_limit(&client_key)?;
    let input = parse_json(input)?;
    let (session, token) = state.create_session(input.nickname).await?;
    let max_age = time::Duration::seconds(state.settings.session_ttl.as_secs() as i64);
    let cookie = Cookie::build(("mk01_session", token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.settings.secure_cookies)
        .max_age(max_age)
        .build();
    let expires_at =
        Utc::now() + ChronoDuration::seconds(state.settings.session_ttl.as_secs() as i64);
    Ok((
        jar.add(cookie),
        (
            StatusCode::CREATED,
            Json(SessionResponse {
                id: session.id,
                nickname: session.nickname,
                current_room_id: session.current_room_id,
                expires_at,
            }),
        ),
    ))
}

async fn delete_current_session(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<impl IntoResponse, GameError> {
    let session = authenticate(&state, &jar, &headers).await?;
    if let Some(room) = state.revoke_session(&session).await? {
        state
            .broadcast_snapshots(&room, SnapshotEvent::PlayerLeft)
            .await;
        state.broadcast_latest_chat_message(&room);
    }
    let removal_cookie = Cookie::build(("mk01_session", "")).path("/").build();
    Ok((jar.remove(removal_cookie), StatusCode::NO_CONTENT))
}

async fn current_session(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<SessionResponse>, GameError> {
    let session = authenticate(&state, &jar, &headers).await?;
    Ok(Json(SessionResponse {
        id: session.id,
        nickname: session.nickname,
        current_room_id: session.current_room_id,
        expires_at: session.last_seen_at
            + ChronoDuration::seconds(state.settings.session_ttl.as_secs() as i64),
    }))
}

async fn list_rooms(State(state): State<AppState>) -> Result<Json<RoomListResponse>, GameError> {
    Ok(Json(RoomListResponse {
        rooms: state.store.list_public_rooms().await?,
        server_time: Utc::now(),
        protocol_version: crate::PROTOCOL_VERSION,
    }))
}

async fn create_room(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    input: Result<Json<CreateRoomInput>, JsonRejection>,
) -> Result<impl IntoResponse, GameError> {
    let input = parse_json(input)?;
    let session = authenticate(&state, &jar, &headers).await?;
    let room = state.create_room(&session, input).await?;
    let snapshot = room.snapshot_for(session.id)?;
    let response = RoomCreatedResponse {
        invite_url: state.invite_url(&room.code),
        snapshot,
    };
    Ok((StatusCode::CREATED, Json(response)))
}

async fn join_room(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    input: Result<Json<JoinRoomInput>, JsonRejection>,
) -> Result<Json<GameSnapshot>, GameError> {
    let input = parse_json(input)?;
    let session = authenticate(&state, &jar, &headers).await?;
    let room = state.join_room(&session, &input.code).await?;
    let snapshot = room.snapshot_for(session.id)?;
    state
        .broadcast_snapshots(&room, SnapshotEvent::PlayerJoined)
        .await;
    state.broadcast_latest_chat_message(&room);
    Ok(Json(snapshot))
}

async fn room_state(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    room_id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<GameSnapshot>, GameError> {
    let Path(room_id) = parse_path(room_id)?;
    let session = authenticate(&state, &jar, &headers).await?;
    let room = state.room(room_id).await?;
    let room = room.lock().await;
    Ok(Json(room.snapshot_for(session.id)?))
}

async fn leave_room(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    room_id: Result<Path<Uuid>, PathRejection>,
) -> Result<StatusCode, GameError> {
    let Path(room_id) = parse_path(room_id)?;
    let session = authenticate(&state, &jar, &headers).await?;
    let room = state.leave_room(&session, room_id).await?;
    state
        .broadcast_snapshots(&room, SnapshotEvent::PlayerLeft)
        .await;
    state.broadcast_latest_chat_message(&room);
    Ok(StatusCode::NO_CONTENT)
}

async fn recover_game(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<Option<GameSnapshot>>, GameError> {
    let session = authenticate(&state, &jar, &headers).await?;
    let Some(room_id) = session.current_room_id else {
        return Ok(Json(None));
    };
    let room = match state.room(room_id).await {
        Ok(room) => room,
        Err(GameError::RoomNotFound) => {
            state.store.update_session_room(session.id, None).await?;
            return Ok(Json(None));
        }
        Err(error) => return Err(error),
    };
    let room = room.lock().await;
    Ok(Json(Some(room.snapshot_for(session.id)?)))
}

async fn game_history(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<HistoryResponse>, GameError> {
    let session = authenticate(&state, &jar, &headers).await?;
    Ok(Json(HistoryResponse {
        games: state.store.history_for_session(session.id).await?,
    }))
}

async fn enqueue_matchmaking(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<MatchmakingResponse>, GameError> {
    let session = authenticate(&state, &jar, &headers).await?;
    let result = state.enqueue_matchmaking(session.clone()).await?;
    let snapshot = result
        .as_ref()
        .map(|room| room.snapshot_for(session.id))
        .transpose()?;
    let queued_at = if snapshot.is_none() {
        state.matchmaking_time(session.id).await
    } else {
        None
    };
    Ok(Json(MatchmakingResponse {
        queued: snapshot.is_none(),
        queued_at,
        snapshot,
    }))
}

async fn cancel_matchmaking(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<StatusCode, GameError> {
    let session = authenticate(&state, &jar, &headers).await?;
    state.cancel_matchmaking(session.id).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryResponse {
    games: Vec<GameHistoryItem>,
}

fn parse_json<T>(input: Result<Json<T>, JsonRejection>) -> Result<T, GameError> {
    input.map(|Json(value)| value).map_err(|rejection| {
        tracing::debug!(%rejection, "request body rejected");
        GameError::InvalidRequest
    })
}

fn parse_path<T>(input: Result<Path<T>, PathRejection>) -> Result<Path<T>, GameError> {
    input.map_err(|rejection| {
        tracing::debug!(%rejection, "request path rejected");
        GameError::InvalidRequest
    })
}

pub async fn authenticate(
    state: &AppState,
    jar: &CookieJar,
    headers: &HeaderMap,
) -> Result<crate::domain::UserSession, GameError> {
    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let session = state.authenticate(jar, authorization).await?;
    state.enforce_api_rate_limit(session.id)?;
    Ok(session)
}

fn client_rate_limit_key(
    headers: &HeaderMap,
    direct_address: Option<SocketAddr>,
    trust_proxy_headers: bool,
) -> String {
    if trust_proxy_headers {
        let forwarded_ip = headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .map(str::trim)
            .and_then(|value| value.parse::<std::net::IpAddr>().ok())
            .or_else(|| {
                headers
                    .get("x-real-ip")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<std::net::IpAddr>().ok())
            });
        if let Some(ip) = forwarded_ip {
            return ip.to_string();
        }
    }
    direct_address
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|| "unknown-client".to_string())
}
