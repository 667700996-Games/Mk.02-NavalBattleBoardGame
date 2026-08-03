use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{
        HeaderMap,
        header::{AUTHORIZATION, ORIGIN},
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
    app::{AppState, SnapshotEvent},
    error::GameError,
    protocol::{ClientEvent, HeartbeatResponse, ProtocolError, RoomCreatedResponse, ServerEvent},
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
    let session = authenticate(&state, &jar, &headers).await?;
    Ok(upgrade
        .max_message_size(64 * 1024)
        .max_frame_size(64 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, state, session)))
}

async fn handle_socket(socket: WebSocket, state: AppState, session: crate::domain::UserSession) {
    let (mut socket_sender, mut socket_receiver) = socket.split();
    let (event_sender, mut event_receiver) = mpsc::unbounded_channel();
    let connection_id = state.hub.connect(session.id, event_sender);
    state.restore_connection(&session).await;

    loop {
        tokio::select! {
            outgoing = event_receiver.recv() => {
                let Some(event) = outgoing else { break };
                let Ok(json) = serde_json::to_string(&event) else { continue };
                if socket_sender.send(Message::Text(json.into())).await.is_err() { break; }
            }
            incoming = socket_receiver.next() => {
                let Some(Ok(message)) = incoming else { break };
                match message {
                    Message::Text(text) => {
                        match serde_json::from_str::<ClientEvent>(&text) {
                            Ok(event) => handle_event(&state, &session, event).await,
                            Err(_) => state.hub.send(session.id, error_event(GameError::InvalidRequest)),
                        }
                    }
                    Message::Ping(payload) => {
                        if socket_sender.send(Message::Pong(payload)).await.is_err() { break; }
                    }
                    Message::Close(_) => break,
                    Message::Binary(_) | Message::Pong(_) => {}
                }
            }
        }
    }

    if state.hub.disconnect_if_current(session.id, connection_id) {
        state.disconnect_session(session.id).await;
    }
}

async fn handle_event(state: &AppState, session: &crate::domain::UserSession, event: ClientEvent) {
    let is_placement_event = matches!(
        &event,
        ClientEvent::ShipsPlace(_) | ClientEvent::ShipsConfirm(_)
    );
    let result = match event {
        ClientEvent::RoomCreate(input) => {
            async {
                let room = state.create_room(session, input).await?;
                let response = RoomCreatedResponse {
                    invite_url: state.invite_url(&room.code),
                    snapshot: room.snapshot_for(session.id)?,
                };
                state
                    .hub
                    .send(session.id, ServerEvent::RoomCreated(response));
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
                Ok(())
            }
            .await
        }
        ClientEvent::PlayerReady(input) => {
            async {
                let room_ref = state.room(input.room_id).await?;
                let mut room = room_ref.lock().await;
                room.set_ready(session.id, input.player_id, input.ready)?;
                state.save_room(&room).await?;
                state
                    .broadcast_snapshots(&room, SnapshotEvent::RoomUpdated)
                    .await;
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
                state.save_room(&room).await?;
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
                let started = room.confirm_placement(session.id)?;
                state.save_room(&room).await?;
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
                    state.save_room(&room).await?;
                }
                if duplicate {
                    state
                        .hub
                        .send(session.id, ServerEvent::AttackResult(record));
                    if let Ok(snapshot) = room.snapshot_for(session.id) {
                        state
                            .hub
                            .send(session.id, ServerEvent::GameSnapshot(snapshot));
                    }
                } else {
                    for player in &room.players {
                        state
                            .hub
                            .send(player.session_id, ServerEvent::AttackResult(record.clone()));
                        if record.sunk_ship.is_some() {
                            state
                                .hub
                                .send(player.session_id, ServerEvent::ShipSunk(record.clone()));
                        }
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
                }
                Ok(())
            }
            .await
        }
        ClientEvent::GameRematch(input) => {
            async {
                let room_ref = state.room(input.room_id).await?;
                let mut room = room_ref.lock().await;
                room.request_rematch(session.id)?;
                state.save_room(&room).await?;
                state
                    .broadcast_snapshots(&room, SnapshotEvent::RoomUpdated)
                    .await;
                Ok(())
            }
            .await
        }
        ClientEvent::GameSync(input) => {
            async {
                let room_ref = state.room(input.room_id).await?;
                let room = room_ref.lock().await;
                state.hub.send(
                    session.id,
                    ServerEvent::GameSnapshot(room.snapshot_for(session.id)?),
                );
                Ok(())
            }
            .await
        }
        ClientEvent::Heartbeat(_) => {
            state.hub.send(
                session.id,
                ServerEvent::Heartbeat(HeartbeatResponse {
                    server_time: Utc::now(),
                }),
            );
            Ok(())
        }
    };
    if let Err(error) = result {
        let protocol_error = protocol_error(error);
        state.hub.send(
            session.id,
            if is_placement_event {
                ServerEvent::PlacementRejected(protocol_error)
            } else {
                ServerEvent::Error(protocol_error)
            },
        );
    }
}

fn error_event(error: GameError) -> ServerEvent {
    ServerEvent::Error(protocol_error(error))
}

fn protocol_error(error: GameError) -> ProtocolError {
    ProtocolError {
        code: error.code().to_string(),
        message: error.to_string(),
        retryable: matches!(
            error,
            GameError::VersionConflict | GameError::TurnConflict | GameError::StorageUnavailable
        ),
        request_id: Uuid::new_v4(),
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
}
