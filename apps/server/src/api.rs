use axum::{
    Json, Router,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::IntoResponse,
    routing::{get, post},
};
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
        .route("/sessions", post(create_session))
        .route("/sessions/current", get(current_session))
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
    })
}

async fn create_session(
    State(state): State<AppState>,
    jar: CookieJar,
    input: Result<Json<CreateSessionInput>, JsonRejection>,
) -> Result<impl IntoResponse, GameError> {
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
    state.authenticate(jar, authorization).await
}
