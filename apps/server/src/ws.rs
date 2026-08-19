use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{
        HeaderMap,
        header::{AUTHORIZATION, ORIGIN, SEC_WEBSOCKET_PROTOCOL},
    },
    response::Response,
};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    api::authenticate,
    app::{AppState, CommandTransport, SnapshotEvent},
    domain::{IntegritySignalKind, QuickCommandId},
    error::GameError,
    protocol::{
        ChatHistoryResponse, ClientEvent, HeartbeatResponse, NegotiatedProtocol, ProtocolError,
        RoomCreatedResponse, ServerEvent, negotiate_protocol_version, websocket_subprotocol,
    },
};

pub async fn websocket_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response, GameError> {
    if !origin_allowed(&state.settings.allowed_origins, &headers) {
        return Err(GameError::OriginNotAllowed);
    }
    let (negotiated, selected_subprotocol) = match negotiate_websocket_protocol(&headers) {
        Ok((negotiated, selected_subprotocol)) => {
            state
                .metrics
                .record_protocol_websocket_negotiation(negotiated.0);
            (negotiated, selected_subprotocol)
        }
        Err(error) => {
            state
                .metrics
                .protocol_websocket_rejections
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(error);
        }
    };
    let session = authenticate(&state, &jar, &headers).await?;
    let connection_permit = state.try_acquire_websocket_slot()?;
    let upgrade = match selected_subprotocol {
        Some(subprotocol) => upgrade.protocols([subprotocol]),
        None => upgrade,
    };
    Ok(upgrade
        .max_message_size(64 * 1024)
        .max_frame_size(64 * 1024)
        .on_upgrade(move |socket| {
            handle_socket(socket, state, session, negotiated, connection_permit)
        }))
}

fn negotiate_websocket_protocol(
    headers: &HeaderMap,
) -> Result<(NegotiatedProtocol, Option<&'static str>), GameError> {
    let Some(header) = headers.get(SEC_WEBSOCKET_PROTOCOL) else {
        return negotiate_protocol_version(None).map(|version| (version, None));
    };
    let offered = header
        .to_str()
        .map_err(|_| GameError::ProtocolVersionMismatch)?;
    for candidate in offered.split(',').map(str::trim) {
        let Some(version) = candidate
            .strip_prefix("mk01.v")
            .and_then(|value| value.parse::<u16>().ok())
        else {
            continue;
        };
        if let Ok(negotiated) = negotiate_protocol_version(Some(version)) {
            let subprotocol =
                websocket_subprotocol(negotiated.0).ok_or(GameError::ProtocolVersionMismatch)?;
            return Ok((negotiated, Some(subprotocol)));
        }
    }
    Err(GameError::ProtocolVersionMismatch)
}

async fn handle_socket(
    socket: WebSocket,
    state: AppState,
    session: crate::domain::UserSession,
    negotiated: NegotiatedProtocol,
    _connection_permit: tokio::sync::OwnedSemaphorePermit,
) {
    let connected_at = std::time::Instant::now();
    let mut unexpected_disconnect = false;
    state
        .metrics
        .websocket_connections
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (mut socket_sender, mut socket_receiver) = socket.split();
    let (event_sender, mut event_receiver) = mpsc::channel(state.websocket_send_queue_capacity());
    let connection_id = state.hub.connect(session.id, negotiated.0, event_sender);
    state.restore_connection(&session).await;

    loop {
        tokio::select! {
            outgoing = event_receiver.recv() => {
                let Some(json) = outgoing else { break };
                if socket_sender.send(Message::Text(json.into())).await.is_err() {
                    unexpected_disconnect = true;
                    break;
                }
            }
            incoming = socket_receiver.next() => {
                let Some(message) = incoming else {
                    unexpected_disconnect = true;
                    break;
                };
                let Ok(message) = message else {
                    unexpected_disconnect = true;
                    break;
                };
                match message {
                    Message::Text(text) => {
                        let command_started_at = std::time::Instant::now();
                        if let Err(error) = state
                            .enforce_websocket_event_rate_limit(session.id)
                            .await
                        {
                            if error == GameError::RateLimited {
                                state
                                    .metrics
                                    .rate_limit_rejections
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                state
                                    .record_integrity_signal(
                                        &session,
                                        session.current_room_id,
                                        IntegritySignalKind::Automation,
                                        3,
                                        0.88,
                                        serde_json::json!({
                                            "detector": "WEBSOCKET_EVENT_BURST",
                                            "eventsPerSecondLimit": state.settings.websocket_events_per_second,
                                            "protocolVersion": crate::PROTOCOL_VERSION,
                                        }),
                                    )
                                    .await;
                            }
                            state.metrics.record_command_latency(
                                CommandTransport::Websocket,
                                false,
                                command_started_at.elapsed(),
                            );
                            state
                                .send_to_session(session.id, error_event(error))
                                .await;
                            continue;
                        }
                        match serde_json::from_str::<ClientEvent>(&text) {
                            Ok(event) => {
                                handle_event(&state, &session, event, command_started_at).await
                            }
                            Err(_) => {
                                state.metrics.record_command_latency(
                                    CommandTransport::Websocket,
                                    false,
                                    command_started_at.elapsed(),
                                );
                                state
                                    .send_to_session(
                                        session.id,
                                        error_event(GameError::InvalidRequest),
                                    )
                                    .await;
                            }
                        }
                    }
                    Message::Ping(payload) => {
                        if socket_sender.send(Message::Pong(payload)).await.is_err() {
                            unexpected_disconnect = true;
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    Message::Binary(_) | Message::Pong(_) => {}
                }
            }
        }
    }

    state
        .metrics
        .record_websocket_disconnect(connected_at.elapsed(), unexpected_disconnect);
    state
        .metrics
        .websocket_connections
        .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);

    if state.hub.disconnect_if_current(session.id, connection_id) {
        state.disconnect_session(session.id).await;
    }
}

async fn handle_event(
    state: &AppState,
    session: &crate::domain::UserSession,
    event: ClientEvent,
    started_at: std::time::Instant,
) {
    state
        .metrics
        .websocket_events
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let is_placement_event = matches!(
        &event,
        ClientEvent::ShipsPlace(_) | ClientEvent::ShipsConfirm(_)
    );
    let is_ready_event = matches!(&event, ClientEvent::PlayerReady(_));
    let is_unready_event = matches!(&event, ClientEvent::PlayerUnready(_));
    let is_game_start_event = matches!(&event, ClientEvent::GameStart(_));
    let is_chat_event = matches!(&event, ClientEvent::ChatSend(_));
    let (integrity_room_id, event_name) = integrity_event_context(&event);
    let client_request_id = match &event {
        ClientEvent::PlayerReady(input) => Some(input.request_id),
        ClientEvent::PlayerUnready(input) => Some(input.request_id),
        ClientEvent::GameStart(input) => Some(input.request_id),
        ClientEvent::AttackFire(input) => Some(input.request_id),
        ClientEvent::ChatSend(input) => Some(input.client_message_id),
        _ => None,
    };
    let result = match event {
        ClientEvent::RoomCreate(input) => {
            async {
                let room = state.create_room(session, input).await?;
                let response = RoomCreatedResponse {
                    invite_url: state.invite_url(&room.code),
                    snapshot: room.snapshot_for(session.id)?,
                };
                state
                    .send_to_session(session.id, ServerEvent::RoomCreated(response))
                    .await;
                Ok(())
            }
            .await
        }
        ClientEvent::RoomJoin(input) => {
            async {
                let room = state.join_room(session, &input.code).await?;
                state
                    .broadcast_snapshots(&room, SnapshotEvent::PlayerJoined)
                    .await;
                state.broadcast_latest_chat_message(&room).await;
                Ok(())
            }
            .await
        }
        ClientEvent::RoomLeave(input) => {
            async {
                let room = state.leave_room(session, input.room_id).await?;
                state
                    .broadcast_snapshots(&room, SnapshotEvent::PlayerLeft)
                    .await;
                state.broadcast_latest_chat_message(&room).await;
                Ok(())
            }
            .await
        }
        ClientEvent::PlayerReady(input) => {
            async {
                let room_ref = state.room(input.room_id).await?;
                let mut room = room_ref.lock().await;
                let previous_version = room.version;
                let chat_start = room.chat_messages.len();
                let (record, duplicate) =
                    room.set_lobby_ready(session.id, input.request_id, input.player_id, true)?;
                if !duplicate {
                    state.save_room(&mut room).await?;
                }
                state
                    .send_to_session(session.id, ServerEvent::PlayerReadyAccepted(record))
                    .await;
                if room.version != previous_version {
                    for message in &room.chat_messages[chat_start..] {
                        state.broadcast_chat_message(&room, message).await;
                    }
                    state
                        .broadcast_snapshots(&room, SnapshotEvent::RoomUpdated)
                        .await;
                }
                Ok(())
            }
            .await
        }
        ClientEvent::ShipsPlace(input) => {
            async {
                let room_ref = state.room(input.room_id).await?;
                let mut room = room_ref.lock().await;
                if room.player_for_session(session.id)?.id != input.player_id {
                    return Err(GameError::Unauthorized);
                }
                room.place_ships(session.id, input.placements)?;
                state.save_room(&mut room).await?;
                state
                    .broadcast_snapshots(&room, SnapshotEvent::PlacementAccepted)
                    .await;
                Ok(())
            }
            .await
        }
        ClientEvent::ShipsConfirm(input) => {
            async {
                let room_ref = state.room(input.room_id).await?;
                let mut room = room_ref.lock().await;
                if room.player_for_session(session.id)?.id != input.player_id {
                    return Err(GameError::Unauthorized);
                }
                let started = room.confirm_placement(
                    session.id,
                    &input.placements,
                    state.settings.turn_duration_seconds,
                )?;
                state.save_room(&mut room).await?;
                let timer = room.timer_state(Utc::now());
                state
                    .broadcast_snapshots(
                        &room,
                        if started {
                            SnapshotEvent::GameStarted
                        } else {
                            SnapshotEvent::PlacementAccepted
                        },
                    )
                    .await;
                if started {
                    state.broadcast_latest_chat_message(&room).await;
                    state
                        .broadcast_timer_state(&room, ServerEvent::TurnStarted)
                        .await;
                    state.schedule_turn_expiry(timer);
                    state.schedule_ai_turn(room.id);
                }
                Ok(())
            }
            .await
        }
        ClientEvent::PlayerUnready(input) => {
            async {
                let room_ref = state.room(input.room_id).await?;
                let mut room = room_ref.lock().await;
                let previous_version = room.version;
                let chat_start = room.chat_messages.len();
                let (record, duplicate) =
                    room.set_lobby_ready(session.id, input.request_id, input.player_id, false)?;
                if !duplicate {
                    state.save_room(&mut room).await?;
                }
                state
                    .send_to_session(session.id, ServerEvent::PlayerUnreadyAccepted(record))
                    .await;
                if room.version != previous_version {
                    for message in &room.chat_messages[chat_start..] {
                        state.broadcast_chat_message(&room, message).await;
                    }
                    state
                        .broadcast_snapshots(&room, SnapshotEvent::RoomUpdated)
                        .await;
                }
                Ok(())
            }
            .await
        }
        ClientEvent::GameStart(input) => {
            async {
                let room_ref = state.room(input.room_id).await?;
                let mut room = room_ref.lock().await;
                let chat_start = room.chat_messages.len();
                let (record, duplicate) = match room.start_placement(
                    session.id,
                    input.request_id,
                    input.player_id,
                    input.room_version,
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        if let Ok(message) = room.record_start_rejection(session.id, error.code()) {
                            state.save_room(&mut room).await?;
                            state.broadcast_chat_message(&room, &message).await;
                        }
                        return Err(error);
                    }
                };
                if !duplicate {
                    state.save_room(&mut room).await?;
                }
                state
                    .send_to_session(session.id, ServerEvent::GameStartAccepted(record))
                    .await;
                if !duplicate {
                    for message in &room.chat_messages[chat_start..] {
                        state.broadcast_chat_message(&room, message).await;
                    }
                    state
                        .broadcast_snapshots(&room, SnapshotEvent::GamePlacementStarted)
                        .await;
                    state
                        .broadcast_snapshots(&room, SnapshotEvent::RoomUpdated)
                        .await;
                    state
                        .broadcast_snapshots(&room, SnapshotEvent::GameSnapshot)
                        .await;
                }
                Ok(())
            }
            .await
        }
        ClientEvent::AttackFire(input) => {
            async {
                let room_ref = state.room(input.room_id).await?;
                let mut room = room_ref.lock().await;
                let (record, duplicate) = room.fire(
                    session.id,
                    input.request_id,
                    input.player_id,
                    input.coordinate,
                    input.expected_version,
                    input.turn_number,
                )?;
                if !duplicate {
                    state.save_room(&mut room).await?;
                }
                if duplicate {
                    state
                        .send_to_session(session.id, ServerEvent::AttackResult(record))
                        .await;
                    if let Ok(snapshot) = room.snapshot_for(session.id) {
                        state
                            .send_to_session(session.id, ServerEvent::GameSnapshot(snapshot))
                            .await;
                    }
                } else {
                    let timer = room.timer_state(Utc::now());
                    for player in &room.players {
                        state
                            .send_to_session(
                                player.session_id,
                                ServerEvent::AttackResult(record.clone()),
                            )
                            .await;
                        if record.sunk_ship.is_some() {
                            state
                                .send_to_session(
                                    player.session_id,
                                    ServerEvent::ShipSunk(record.clone()),
                                )
                                .await;
                        }
                    }
                    if record.winner_id.is_some() {
                        state.broadcast_latest_chat_message(&room).await;
                    }
                    state
                        .broadcast_snapshots(
                            &room,
                            if record.winner_id.is_some() {
                                SnapshotEvent::GameFinished
                            } else {
                                SnapshotEvent::TurnChanged
                            },
                        )
                        .await;
                    if record.winner_id.is_some() {
                        state.cancel_turn_expiry(room.id);
                    } else {
                        state
                            .broadcast_timer_state(&room, ServerEvent::TurnStarted)
                            .await;
                        state.schedule_turn_expiry(timer);
                        state.schedule_ai_turn(room.id);
                    }
                }
                Ok(())
            }
            .await
        }
        ClientEvent::GameSurrender(input) => {
            async {
                let room_ref = state.room(input.room_id).await?;
                let mut room = room_ref.lock().await;
                let record = room.surrender(session.id, input.player_id)?;
                state.save_room(&mut room).await?;
                state.cancel_turn_expiry(room.id);
                for player in &room.players {
                    state
                        .send_to_session(
                            player.session_id,
                            ServerEvent::GameSurrendered(record.clone()),
                        )
                        .await;
                }
                state.broadcast_latest_chat_message(&room).await;
                state
                    .broadcast_snapshots(&room, SnapshotEvent::GameFinished)
                    .await;
                Ok(())
            }
            .await
        }
        ClientEvent::ChatSend(input) => {
            async {
                let room_ref = state.room(input.room_id).await?;
                let mut room = room_ref.lock().await;
                let command_id = match input.command_id.as_deref() {
                    Some(value) => Some(
                        QuickCommandId::from_wire(value).ok_or(GameError::InvalidQuickCommand)?,
                    ),
                    None => None,
                };
                let (message, duplicate) = room.send_chat(
                    session.id,
                    input.client_message_id,
                    input.message_type,
                    input.content,
                    command_id,
                )?;
                if duplicate {
                    state
                        .send_to_session(session.id, ServerEvent::ChatMessage(message))
                        .await;
                } else {
                    state.save_room(&mut room).await?;
                    state.broadcast_chat_message(&room, &message).await;
                }
                Ok(())
            }
            .await
        }
        ClientEvent::ChatTyping(input) => {
            async {
                let room_ref = state.room(input.room_id).await?;
                let room = room_ref.lock().await;
                let event = room.typing_event(session.id, input.is_typing)?;
                state.broadcast_chat_typing(&room, &event).await;
                Ok(())
            }
            .await
        }
        ClientEvent::GameSync(input) => {
            async {
                let room_ref = state.room(input.room_id).await?;
                let room = room_ref.lock().await;
                state
                    .send_to_session(
                        session.id,
                        ServerEvent::GameSnapshot(room.snapshot_for(session.id)?),
                    )
                    .await;
                state
                    .send_to_session(
                        session.id,
                        ServerEvent::ChatHistory(ChatHistoryResponse {
                            room_id: room.id,
                            messages: state.chat_history_for(&room, session.id).await?,
                        }),
                    )
                    .await;
                if let Some(timer) = room.timer_state(Utc::now()) {
                    state
                        .send_to_session(session.id, ServerEvent::GameTimerSync(timer))
                        .await;
                }
                Ok(())
            }
            .await
        }
        ClientEvent::Heartbeat(_) => {
            state
                .send_to_session(
                    session.id,
                    ServerEvent::Heartbeat(HeartbeatResponse {
                        server_time: Utc::now(),
                    }),
                )
                .await;
            Ok(())
        }
    };
    state.metrics.record_command_latency(
        CommandTransport::Websocket,
        result.is_ok(),
        started_at.elapsed(),
    );
    if let Err(error) = result {
        tracing::warn!(
            session_id = %session.id,
            error_code = error.code(),
            request_id = ?client_request_id,
            "websocket request rejected"
        );
        if is_impossible_order_error(&error) {
            state
                .record_integrity_signal(
                    session,
                    integrity_room_id,
                    IntegritySignalKind::ImpossibleOrder,
                    if error == GameError::Unauthorized {
                        4
                    } else {
                        2
                    },
                    if error == GameError::Unauthorized {
                        0.96
                    } else {
                        0.72
                    },
                    serde_json::json!({
                        "detector": "AUTHORITATIVE_COMMAND_REJECTION",
                        "event": event_name,
                        "errorCode": error.code(),
                        "requestId": client_request_id,
                        "protocolVersion": crate::PROTOCOL_VERSION,
                    }),
                )
                .await;
        }
        let protocol_error = protocol_error(error, client_request_id);
        state
            .send_to_session(
                session.id,
                if is_ready_event {
                    ServerEvent::PlayerReadyRejected(protocol_error)
                } else if is_unready_event {
                    ServerEvent::PlayerUnreadyRejected(protocol_error)
                } else if is_game_start_event {
                    ServerEvent::GameStartRejected(protocol_error)
                } else if is_chat_event {
                    ServerEvent::ChatRejected(protocol_error)
                } else if is_placement_event {
                    ServerEvent::PlacementRejected(protocol_error)
                } else {
                    ServerEvent::Error(protocol_error)
                },
            )
            .await;
    }
}

fn is_impossible_order_error(error: &GameError) -> bool {
    matches!(
        error,
        GameError::NotYourTurn
            | GameError::CoordinateAlreadyAttacked
            | GameError::TurnConflict
            | GameError::StaleRoomVersion
            | GameError::PlacementMismatch
            | GameError::Unauthorized
    )
}

fn integrity_event_context(event: &ClientEvent) -> (Option<Uuid>, &'static str) {
    match event {
        ClientEvent::RoomCreate(_) => (None, "room:create"),
        ClientEvent::RoomJoin(_) => (None, "room:join"),
        ClientEvent::RoomLeave(input) => (Some(input.room_id), "room:leave"),
        ClientEvent::PlayerReady(input) => (Some(input.room_id), "player:ready"),
        ClientEvent::ShipsPlace(input) => (Some(input.room_id), "ships:place"),
        ClientEvent::ShipsConfirm(input) => (Some(input.room_id), "ships:confirm"),
        ClientEvent::PlayerUnready(input) => (Some(input.room_id), "player:unready"),
        ClientEvent::GameStart(input) => (Some(input.room_id), "game:start"),
        ClientEvent::AttackFire(input) => (Some(input.room_id), "attack:fire"),
        ClientEvent::GameSurrender(input) => (Some(input.room_id), "game:surrender"),
        ClientEvent::ChatSend(input) => (Some(input.room_id), "chat:send"),
        ClientEvent::ChatTyping(input) => (Some(input.room_id), "chat:typing"),
        ClientEvent::GameSync(input) => (Some(input.room_id), "game:sync"),
        ClientEvent::Heartbeat(_) => (None, "heartbeat"),
    }
}

fn error_event(error: GameError) -> ServerEvent {
    ServerEvent::Error(protocol_error(error, None))
}

fn protocol_error(error: GameError, request_id: Option<Uuid>) -> ProtocolError {
    ProtocolError {
        code: error.code().to_string(),
        message: error.to_string(),
        retryable: matches!(
            error,
            GameError::VersionConflict
                | GameError::TurnConflict
                | GameError::TurnExpired
                | GameError::StaleRoomVersion
                | GameError::StorageUnavailable
        ),
        request_id: request_id.unwrap_or_else(Uuid::new_v4),
    }
}

fn origin_allowed(allowed_origins: &[String], headers: &HeaderMap) -> bool {
    headers
        .get(ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(|origin| allowed_origins.iter().any(|allowed| allowed == origin))
        .unwrap_or_else(|| headers.contains_key(AUTHORIZATION))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn websocket_origin_must_be_allowlisted_or_use_bearer_auth() {
        let allowed = vec!["https://game.example.com".to_string()];
        let mut headers = HeaderMap::new();
        headers.insert(ORIGIN, HeaderValue::from_static("https://game.example.com"));
        assert!(origin_allowed(&allowed, &headers));

        headers.insert(ORIGIN, HeaderValue::from_static("https://evil.example"));
        assert!(!origin_allowed(&allowed, &headers));

        headers.remove(ORIGIN);
        assert!(!origin_allowed(&allowed, &headers));
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer test"));
        assert!(origin_allowed(&allowed, &headers));
    }

    #[test]
    fn websocket_protocol_negotiation_supports_headerless_v3_and_rejects_unknown_versions() {
        let mut headers = HeaderMap::new();
        assert_eq!(
            negotiate_websocket_protocol(&headers).unwrap(),
            (NegotiatedProtocol(3), None)
        );

        headers.insert(
            SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_static("mk01.v2, mk01.v3"),
        );
        assert_eq!(
            negotiate_websocket_protocol(&headers).unwrap(),
            (NegotiatedProtocol(3), Some("mk01.v3"))
        );

        headers.insert(
            SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_static("mk01.v1, mk01.v4"),
        );
        assert_eq!(
            negotiate_websocket_protocol(&headers).unwrap_err(),
            GameError::ProtocolVersionMismatch
        );
    }

    #[test]
    fn impossible_order_detector_targets_authoritative_abuse_without_flagging_retries() {
        for error in [
            GameError::NotYourTurn,
            GameError::CoordinateAlreadyAttacked,
            GameError::TurnConflict,
            GameError::StaleRoomVersion,
            GameError::PlacementMismatch,
            GameError::Unauthorized,
        ] {
            assert!(is_impossible_order_error(&error));
        }
        assert!(!is_impossible_order_error(&GameError::VersionConflict));
        assert!(!is_impossible_order_error(&GameError::StorageUnavailable));
        assert!(!is_impossible_order_error(&GameError::RateLimited));
    }
}
