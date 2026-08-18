use axum::{
    Json, Router,
    extract::{
        ConnectInfo, Path, Query, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::IntoResponse,
    routing::{delete, get, post},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use chrono::{Duration as ChronoDuration, Utc};
use serde::Serialize;
use std::net::SocketAddr;
use uuid::Uuid;

use crate::{
    app::{AppState, SnapshotEvent},
    domain::{GameSnapshot, IntegritySignalPage, ModerationCasePage, PlayerProgression},
    error::GameError,
    protocol::{
        AccountDeletionInput, AccountDeletionResponse, AccountLoginInput, AccountSessionsResponse,
        AccountUpgradeInput, AccountUpgradeResponse, CreatePracticeInput, CreateRoomInput,
        CreateSessionInput, HealthResponse, IntegritySignalQuery, JoinRoomInput,
        MatchmakingResponse, ModerationActionInput, ModerationActionResponse,
        ModerationReportQuery, PlayerReportInput, PlayerReportResponse, RoomCreatedResponse,
        RoomListResponse, SessionResponse, SocialRelationshipInput, SocialRelationshipsResponse,
    },
    store::GameHistoryItem,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(readiness))
        .route("/metrics", get(metrics))
        .route("/sessions", post(create_session))
        .route("/accounts/upgrade", post(upgrade_account))
        .route("/accounts/login", post(login_account))
        .route("/accounts", delete(delete_account))
        .route("/accounts/export", get(export_account_data))
        .route("/accounts/sessions", get(account_sessions))
        .route(
            "/accounts/sessions/{session_id}",
            delete(revoke_account_session),
        )
        .route("/profile", get(player_profile))
        .route(
            "/profile/missions/{mission_id}/claim",
            post(claim_mission_reward),
        )
        .route(
            "/social/relationships",
            get(social_relationships).post(update_social_relationship),
        )
        .route("/reports", post(report_player))
        .route("/admin/moderation/reports", get(moderation_reports))
        .route("/admin/integrity/signals", get(integrity_signals))
        .route(
            "/admin/moderation/reports/{report_id}/actions",
            post(apply_moderation_action),
        )
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
        .route("/games/{room_id}/replay", get(game_replay))
        .route("/practice", post(create_practice))
        .route(
            "/matchmaking",
            post(enqueue_matchmaking).delete(cancel_matchmaking),
        )
}

async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let matchmaking = state
        .store
        .matchmaking_queue_stats()
        .await
        .unwrap_or_default();
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        state.metrics.render_prometheus(matchmaking),
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
    state.health_check().await?;
    Ok(Json(HealthResponse {
        status: "ready",
        storage: state.store.kind(),
        server_time: Utc::now(),
        protocol_version: crate::PROTOCOL_VERSION,
    }))
}

async fn create_session(
    State(state): State<AppState>,
    ConnectInfo(direct_address): ConnectInfo<SocketAddr>,
    jar: CookieJar,
    headers: HeaderMap,
    input: Result<Json<CreateSessionInput>, JsonRejection>,
) -> Result<impl IntoResponse, GameError> {
    let client_key = state.client_rate_limit_key(&headers, direct_address);
    if let Err(error) = state.enforce_session_creation_rate_limit(&client_key).await {
        if error == GameError::RateLimited {
            state
                .metrics
                .rate_limit_rejections
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        return Err(error);
    }
    let input = parse_json(input)?;
    let (session, token) = state.create_session(input.nickname).await?;
    let cookie = session_cookie(&state, token);
    let expires_at =
        Utc::now() + ChronoDuration::seconds(state.settings.session_ttl.as_secs() as i64);
    Ok((
        jar.add(cookie),
        (
            StatusCode::CREATED,
            Json(SessionResponse {
                id: session.id,
                account_id: session.account_id,
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
        state.broadcast_latest_chat_message(&room).await;
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
        account_id: session.account_id,
        nickname: session.nickname,
        current_room_id: session.current_room_id,
        expires_at: session.last_seen_at
            + ChronoDuration::seconds(state.settings.session_ttl.as_secs() as i64),
    }))
}

async fn upgrade_account(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    input: Result<Json<AccountUpgradeInput>, JsonRejection>,
) -> Result<impl IntoResponse, GameError> {
    let input = parse_json(input)?;
    let session = authenticate(&state, &jar, &headers).await?;
    let (account, recovery_key, session_token) =
        state.upgrade_account(&session, input.handle).await?;
    Ok((
        jar.add(session_cookie(&state, session_token)),
        Json(AccountUpgradeResponse {
            account,
            recovery_key,
        }),
    ))
}

async fn login_account(
    State(state): State<AppState>,
    ConnectInfo(direct_address): ConnectInfo<SocketAddr>,
    jar: CookieJar,
    headers: HeaderMap,
    input: Result<Json<AccountLoginInput>, JsonRejection>,
) -> Result<impl IntoResponse, GameError> {
    let client_key = state.client_rate_limit_key(&headers, direct_address);
    state
        .enforce_session_creation_rate_limit(&client_key)
        .await?;
    let input = parse_json(input)?;
    let (session, token) = state
        .login_account(input.account_id, input.recovery_key)
        .await?;
    let expires_at =
        Utc::now() + ChronoDuration::seconds(state.settings.session_ttl.as_secs() as i64);
    Ok((
        jar.add(session_cookie(&state, token)),
        (
            StatusCode::CREATED,
            Json(SessionResponse {
                id: session.id,
                account_id: session.account_id,
                nickname: session.nickname,
                current_room_id: session.current_room_id,
                expires_at,
            }),
        ),
    ))
}

async fn account_sessions(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<AccountSessionsResponse>, GameError> {
    let session = authenticate(&state, &jar, &headers).await?;
    Ok(Json(AccountSessionsResponse {
        current_session_id: session.id,
        sessions: state.account_sessions(&session).await?,
    }))
}

async fn export_account_data(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, GameError> {
    let session = authenticate(&state, &jar, &headers).await?;
    Ok(Json(state.export_account_data(&session).await?))
}

async fn delete_account(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    input: Result<Json<AccountDeletionInput>, JsonRejection>,
) -> Result<impl IntoResponse, GameError> {
    let input = parse_json(input)?;
    let session = authenticate(&state, &jar, &headers).await?;
    let (request_id, deleted_at, stats) = state
        .delete_account(&session, input.recovery_key, input.confirmation)
        .await?;
    let removal_cookie = Cookie::build(("mk01_session", "")).path("/").build();
    Ok((
        jar.remove(removal_cookie),
        Json(AccountDeletionResponse {
            request_id,
            deleted_at,
            stats,
        }),
    ))
}

async fn revoke_account_session(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(session_id): Path<Uuid>,
) -> Result<StatusCode, GameError> {
    let session = authenticate(&state, &jar, &headers).await?;
    if state.revoke_account_session(&session, session_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(GameError::Unauthorized)
    }
}

async fn player_profile(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<PlayerProgression>, GameError> {
    let session = authenticate(&state, &jar, &headers).await?;
    Ok(Json(state.progression(&session).await?))
}

async fn claim_mission_reward(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(mission_id): Path<String>,
) -> Result<Json<PlayerProgression>, GameError> {
    let session = authenticate(&state, &jar, &headers).await?;
    Ok(Json(
        state.claim_mission_reward(&session, &mission_id).await?,
    ))
}

async fn social_relationships(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<SocialRelationshipsResponse>, GameError> {
    let session = authenticate(&state, &jar, &headers).await?;
    Ok(Json(SocialRelationshipsResponse {
        relationships: state.social_relationships(&session).await?,
    }))
}

async fn update_social_relationship(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    input: Result<Json<SocialRelationshipInput>, JsonRejection>,
) -> Result<Json<crate::domain::SocialRelationship>, GameError> {
    let input = parse_json(input)?;
    let session = authenticate(&state, &jar, &headers).await?;
    Ok(Json(
        state
            .update_social_relationship(
                &session,
                input.room_id,
                input.target_player_id,
                input.muted,
                input.blocked,
            )
            .await?,
    ))
}

async fn report_player(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    input: Result<Json<PlayerReportInput>, JsonRejection>,
) -> Result<impl IntoResponse, GameError> {
    let input = parse_json(input)?;
    let session = authenticate(&state, &jar, &headers).await?;
    Ok((
        StatusCode::CREATED,
        Json(PlayerReportResponse {
            report: state
                .report_player(
                    &session,
                    input.room_id,
                    input.target_player_id,
                    input.category,
                    input.details,
                )
                .await?,
        }),
    ))
}

async fn moderation_reports(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ModerationReportQuery>,
) -> Result<Json<ModerationCasePage>, GameError> {
    authenticate_operator(&state, &headers, false)?;
    Ok(Json(
        state
            .moderation_cases(query.search, query.status, query.before, query.limit)
            .await?,
    ))
}

async fn integrity_signals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<IntegritySignalQuery>,
) -> Result<Json<IntegritySignalPage>, GameError> {
    authenticate_operator(&state, &headers, false)?;
    Ok(Json(
        state
            .integrity_signals(query.search, query.kind, query.before, query.limit)
            .await?,
    ))
}

async fn apply_moderation_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(report_id): Path<Uuid>,
    input: Result<Json<ModerationActionInput>, JsonRejection>,
) -> Result<Json<ModerationActionResponse>, GameError> {
    let input = parse_json(input)?;
    let operator_id = authenticate_operator(&state, &headers, true)?;
    Ok(Json(ModerationActionResponse {
        action: state
            .moderate_player_report(
                operator_id,
                report_id,
                input.action,
                input.reason,
                input.duration_hours,
                input.reverses_action_id,
            )
            .await?,
    }))
}

fn authenticate_operator(
    state: &AppState,
    headers: &HeaderMap,
    require_operator_id: bool,
) -> Result<String, GameError> {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(GameError::Unauthorized)?;
    state.authorize_operator(token)?;
    let operator_id = headers
        .get("x-operator-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .trim()
        .to_string();
    if require_operator_id && operator_id.is_empty() {
        return Err(GameError::InvalidRequest);
    }
    Ok(operator_id)
}

fn session_cookie(state: &AppState, token: String) -> Cookie<'static> {
    let max_age = time::Duration::seconds(state.settings.session_ttl.as_secs() as i64);
    Cookie::build(("mk01_session", token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.settings.secure_cookies)
        .max_age(max_age)
        .build()
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

async fn create_practice(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    input: Result<Json<CreatePracticeInput>, JsonRejection>,
) -> Result<Json<GameSnapshot>, GameError> {
    let input = parse_json(input)?;
    let session = authenticate(&state, &jar, &headers).await?;
    Ok(Json(
        state
            .create_practice_room(&session, input.difficulty)
            .await?
            .snapshot_for(session.id)?,
    ))
}

async fn game_replay(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(room_id): Path<Uuid>,
) -> Result<Json<crate::domain::GameReplay>, GameError> {
    let session = authenticate(&state, &jar, &headers).await?;
    let room = state
        .store
        .room_by_id_authoritative(room_id)
        .await?
        .ok_or(GameError::RoomNotFound)?;
    Ok(Json(room.replay_for(session.id)?))
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
    state.broadcast_latest_chat_message(&room).await;
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
    state.broadcast_latest_chat_message(&room).await;
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
    let mut games = state.store.history_for_session(session.id).await?;
    games.truncate(50);
    Ok(Json(HistoryResponse { games }))
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
        state.matchmaking_time(session.id).await?
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
    state.cancel_matchmaking(session.id).await?;
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
    if let Err(error) = state.enforce_api_rate_limit(session.id).await {
        if error == GameError::RateLimited {
            state
                .metrics
                .rate_limit_rejections
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        return Err(error);
    }
    Ok(session)
}
