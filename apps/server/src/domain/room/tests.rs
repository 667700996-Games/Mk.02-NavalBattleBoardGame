use super::*;
use crate::domain::Orientation;

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
    vec![
        ShipPlacement {
            kind: ShipKind::Carrier,
            origin: Coordinate {
                row: first_row,
                col: 0,
            },
            orientation: Orientation::Horizontal,
        },
        ShipPlacement {
            kind: ShipKind::Battleship,
            origin: Coordinate {
                row: first_row + 1,
                col: 0,
            },
            orientation: Orientation::Horizontal,
        },
        ShipPlacement {
            kind: ShipKind::Cruiser,
            origin: Coordinate {
                row: first_row + 2,
                col: 0,
            },
            orientation: Orientation::Horizontal,
        },
        ShipPlacement {
            kind: ShipKind::Submarine,
            origin: Coordinate {
                row: first_row + 3,
                col: 0,
            },
            orientation: Orientation::Horizontal,
        },
        ShipPlacement {
            kind: ShipKind::Destroyer,
            origin: Coordinate {
                row: first_row + 4,
                col: 0,
            },
            orientation: Orientation::Horizontal,
        },
    ]
}

fn playing_room() -> (GameRoom, UserSession, UserSession) {
    let first = session("Alpha");
    let second = session("Bravo");
    let mut room = GameRoom::new(
        "ABC234".to_string(),
        "Test operation".to_string(),
        RoomVisibility::Private,
        &first,
    )
    .unwrap();
    room.join(&second).unwrap();
    prepare_placement(&mut room, &first, &second);
    room.place_ships(first.id, fleet(0)).unwrap();
    room.place_ships(second.id, fleet(5)).unwrap();
    assert!(!room.confirm_placement(first.id, &fleet(0), 60).unwrap());
    assert!(room.confirm_placement(second.id, &fleet(5), 60).unwrap());
    (room, first, second)
}

fn prepare_placement(room: &mut GameRoom, host: &UserSession, guest: &UserSession) {
    let host_player_id = room.player_for_session(host.id).unwrap().id;
    let guest_player_id = room.player_for_session(guest.id).unwrap().id;
    room.set_lobby_ready(host.id, Uuid::new_v4(), host_player_id, true)
        .unwrap();
    room.set_lobby_ready(guest.id, Uuid::new_v4(), guest_player_id, true)
        .unwrap();
    room.start_placement(host.id, Uuid::new_v4(), host_player_id, room.version)
        .unwrap();
}

#[test]
fn follows_waiting_ready_start_placement_playing_state_machine() {
    let host = session("Alpha");
    let guest = session("Bravo");
    let mut room = GameRoom::new(
        "ABC234".to_string(),
        "Test operation".to_string(),
        RoomVisibility::Public,
        &host,
    )
    .unwrap();
    assert_eq!(room.status, RoomStatus::WaitingForOpponent);
    room.join(&guest).unwrap();
    assert_eq!(room.status, RoomStatus::WaitingForReady);
    let host_player_id = room.player_for_session(host.id).unwrap().id;
    let guest_player_id = room.player_for_session(guest.id).unwrap().id;
    room.set_lobby_ready(host.id, Uuid::new_v4(), host_player_id, true)
        .unwrap();
    assert_eq!(room.status, RoomStatus::WaitingForReady);
    room.set_lobby_ready(guest.id, Uuid::new_v4(), guest_player_id, true)
        .unwrap();
    assert_eq!(room.status, RoomStatus::ReadyToStart);
    assert!(room.game_id.is_none());
    assert!(room.game.is_none());
    assert!(room.snapshot_for(host.id).unwrap().can_start_game);
    assert!(!room.snapshot_for(guest.id).unwrap().can_start_game);
    room.start_placement(host.id, Uuid::new_v4(), host_player_id, room.version)
        .unwrap();
    assert_eq!(room.status, RoomStatus::Placement);
    assert_eq!(
        room.confirm_placement(host.id, &fleet(0), 60).unwrap_err(),
        GameError::IncompleteFleet
    );
    room.place_ships(host.id, fleet(0)).unwrap();
    room.place_ships(guest.id, fleet(5)).unwrap();
    assert!(!room.confirm_placement(host.id, &fleet(0), 60).unwrap());
    assert!(room.confirm_placement(guest.id, &fleet(5), 60).unwrap());
    assert_eq!(room.status, RoomStatus::Playing);
    assert!(room.game.is_some());
    assert_eq!(
        room.place_ships(host.id, fleet(0)).unwrap_err(),
        GameError::InvalidState
    );
}

#[test]
fn lobby_departures_reset_a_guest_slot_and_host_departure_cancels_the_room() {
    let host = session("Alpha");
    let guest = session("Bravo");
    let mut room = GameRoom::new(
        "ABC234".to_string(),
        "Test operation".to_string(),
        RoomVisibility::Private,
        &host,
    )
    .unwrap();
    let host_player_id = room.player_for_session(host.id).unwrap().id;
    room.set_lobby_ready(host.id, Uuid::new_v4(), host_player_id, true)
        .unwrap();
    assert_eq!(room.status, RoomStatus::WaitingForOpponent);
    assert_eq!(
        room.start_placement(host.id, Uuid::new_v4(), host_player_id, room.version)
            .unwrap_err(),
        GameError::PlayerCountInvalid
    );
    room.join(&guest).unwrap();
    room.leave(guest.id).unwrap();
    assert_eq!(room.status, RoomStatus::WaitingForOpponent);
    assert_eq!(room.players.len(), 1);
    assert_eq!(
        room.player_for_session(host.id).unwrap().ready_state,
        PlayerReadyState::NotReady
    );

    room.join(&guest).unwrap();
    room.leave(host.id).unwrap();
    assert_eq!(room.status, RoomStatus::Cancelled);
    assert_eq!(
        room.chat_messages.last().unwrap().content,
        "방장이 작전실을 종료했습니다."
    );
}

#[tokio::test]
async fn concurrent_start_and_unready_allow_only_one_state_transition() {
    let host = session("Alpha");
    let guest = session("Bravo");
    let mut room = GameRoom::new(
        "ABC234".to_string(),
        "Test operation".to_string(),
        RoomVisibility::Private,
        &host,
    )
    .unwrap();
    room.join(&guest).unwrap();
    let host_player_id = room.player_for_session(host.id).unwrap().id;
    let guest_player_id = room.player_for_session(guest.id).unwrap().id;
    room.set_lobby_ready(host.id, Uuid::new_v4(), host_player_id, true)
        .unwrap();
    room.set_lobby_ready(guest.id, Uuid::new_v4(), guest_player_id, true)
        .unwrap();
    let shared = std::sync::Arc::new(tokio::sync::Mutex::new(room));
    let expected_version = shared.lock().await.version;

    let start_room = shared.clone();
    let start = tokio::spawn(async move {
        start_room.lock().await.start_placement(
            host.id,
            Uuid::new_v4(),
            host_player_id,
            expected_version,
        )
    });
    let unready_room = shared.clone();
    let unready = tokio::spawn(async move {
        unready_room
            .lock()
            .await
            .set_lobby_ready(guest.id, Uuid::new_v4(), guest_player_id, false)
    });

    let start_result = start.await.unwrap();
    let unready_result = unready.await.unwrap();
    assert_ne!(start_result.is_ok(), unready_result.is_ok());
    let room = shared.lock().await;
    if start_result.is_ok() {
        assert_eq!(room.status, RoomStatus::Placement);
        assert_eq!(unready_result.unwrap_err(), GameError::GameAlreadyStarted);
    } else {
        assert_eq!(room.status, RoomStatus::WaitingForReady);
        assert_eq!(start_result.unwrap_err(), GameError::PlayersNotReady);
    }
}

#[test]
fn legacy_auto_placement_state_is_migrated_back_to_the_lobby() {
    let host = session("Alpha");
    let guest = session("Bravo");
    let mut room = GameRoom::new(
        "ABC234".to_string(),
        "Test operation".to_string(),
        RoomVisibility::Private,
        &host,
    )
    .unwrap();
    room.join(&guest).unwrap();
    room.status = serde_json::from_str::<RoomStatus>("\"DISCONNECTED\"").unwrap();
    room.pending_placements.insert(room.players[0].id, fleet(0));
    room.players[0].placement_confirmed = true;
    room.players[0].ready_state = PlayerReadyState::Ready;
    room.players[0].ready_at = Some(Utc::now());

    assert!(room.ensure_runtime_state(60, Utc::now()));
    assert_eq!(room.status, RoomStatus::WaitingForReady);
    assert!(room.pending_placements.is_empty());
    assert!(room.game_id.is_none());
    assert!(room.players.iter().all(|player| {
        player.ready_state == PlayerReadyState::NotReady
            && !player.placement_confirmed
            && player.ready_at.is_none()
    }));
}

#[test]
fn personalized_snapshot_never_contains_opponent_ships_or_session_ids() {
    let (room, first, second) = playing_room();
    let first_snapshot = serde_json::to_value(room.snapshot_for(first.id).unwrap()).unwrap();
    let second_snapshot = serde_json::to_value(room.snapshot_for(second.id).unwrap()).unwrap();

    assert_eq!(first_snapshot["protocolVersion"], crate::PROTOCOL_VERSION);
    assert!(first_snapshot["ownBoard"]["ships"].is_array());
    assert!(second_snapshot["ownBoard"]["ships"].is_array());
    assert!(first_snapshot["targetBoard"].get("ships").is_none());
    assert!(second_snapshot["targetBoard"].get("ships").is_none());
    let first_json = serde_json::to_string(&first_snapshot).unwrap();
    assert!(!first_json.contains("sessionId"));
    assert!(!first_json.contains(&second.id.to_string()));
}

#[test]
fn duplicate_attack_is_idempotent_even_with_stale_version() {
    let (mut room, first, second) = playing_room();
    let current_id = room.game.as_ref().unwrap().current_player_id;
    let (session_id, player_id) = if room.player_for_session(first.id).unwrap().id == current_id {
        (first.id, current_id)
    } else {
        (second.id, current_id)
    };
    let request_id = Uuid::new_v4();
    let version = room.version;
    let (original, duplicate) = room
        .fire(
            session_id,
            request_id,
            player_id,
            Coordinate { row: 9, col: 9 },
            version,
            1,
        )
        .unwrap();
    assert!(!duplicate);
    let resolved_version = room.version;
    let (replayed, duplicate) = room
        .fire(
            session_id,
            request_id,
            player_id,
            Coordinate { row: 9, col: 9 },
            version,
            1,
        )
        .unwrap();
    assert!(duplicate);
    assert_eq!(original.request_id, replayed.request_id);
    assert_eq!(room.version, resolved_version);
}

#[test]
fn reconnect_restores_the_previous_state_and_expiry_forfeits() {
    let (mut room, first, second) = playing_room();
    room.disconnect(first.id, 90).unwrap();
    assert_eq!(room.status, RoomStatus::Playing);
    assert_eq!(
        room.player_for_session(first.id).unwrap().connection_state,
        ConnectionState::Reconnecting
    );
    room.reconnect(first.id).unwrap();
    assert_eq!(room.status, RoomStatus::Playing);

    let first_player_id = room.player_for_session(first.id).unwrap().id;
    let second_player_id = room.player_for_session(second.id).unwrap().id;
    room.disconnect(first.id, 0).unwrap();
    assert!(
        room.expire_disconnect(first_player_id, Utc::now() + Duration::seconds(1))
            .unwrap()
    );
    assert_eq!(room.status, RoomStatus::Finished);
    assert_eq!(
        room.game
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .winner_id,
        second_player_id
    );
    assert_eq!(
        room.game
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .win_type,
        crate::domain::WinType::Disconnect
    );
    assert_eq!(
        room.disconnect(second.id, 90).unwrap_err(),
        GameError::InvalidState
    );
}

#[test]
fn surrender_finishes_once_and_records_the_win_type() {
    let (mut room, first, second) = playing_room();
    let first_player_id = room.player_for_session(first.id).unwrap().id;
    let second_player_id = room.player_for_session(second.id).unwrap().id;

    assert_eq!(
        room.surrender(first.id, second_player_id).unwrap_err(),
        GameError::Unauthorized
    );
    let record = room.surrender(first.id, first_player_id).unwrap();
    assert_eq!(record.surrendered_player_id, first_player_id);
    assert_eq!(record.winner_id, second_player_id);
    assert_eq!(room.status, RoomStatus::Finished);
    let result = room.game.as_ref().unwrap().result.as_ref().unwrap();
    assert_eq!(result.finish_reason, FinishReason::Surrender);
    assert_eq!(result.win_type, crate::domain::WinType::Surrender);
    assert!(
        room.chat_messages
            .last()
            .unwrap()
            .content
            .contains("Commander Alpha surrendered")
    );
    assert_eq!(
        room.surrender(first.id, first_player_id).unwrap_err(),
        GameError::InvalidState
    );
    let (post_game_signal, duplicate) = room
        .send_chat(
            second.id,
            Uuid::new_v4(),
            ChatMessageType::QuickCommand,
            None,
            Some(QuickCommandId::GoodGame),
        )
        .unwrap();
    assert!(!duplicate);
    assert_eq!(post_game_signal.content, "굿게임");
}

#[test]
fn finished_replay_is_versioned_ordered_and_participant_only() {
    let (mut room, first, second) = playing_room();
    let active_player_id = room.game.as_ref().unwrap().current_player_id;
    let active_session_id = if room.player_for_session(first.id).unwrap().id == active_player_id {
        first.id
    } else {
        second.id
    };
    let version = room.version;
    let turn = room.game.as_ref().unwrap().turn_number;
    room.fire(
        active_session_id,
        Uuid::new_v4(),
        active_player_id,
        Coordinate { row: 9, col: 9 },
        version,
        turn,
    )
    .unwrap();
    let surrendering_player_id = room.game.as_ref().unwrap().current_player_id;
    let surrendering_session_id =
        if room.player_for_session(first.id).unwrap().id == surrendering_player_id {
            first.id
        } else {
            second.id
        };
    room.surrender(surrendering_session_id, surrendering_player_id)
        .unwrap();

    let replay = room.replay_for(first.id).unwrap();
    assert_eq!(replay.protocol_version, crate::PROTOCOL_VERSION);
    assert_eq!(replay.ruleset_version, room.balance.ruleset_version);
    assert_eq!(replay.balance, room.balance);
    assert!(replay.balance.has_valid_integrity());
    assert_eq!(replay.balance.manifest.board_size, 10);
    assert_eq!(replay.balance.manifest.consecutive_timeout_forfeit, 3);
    assert_eq!(replay.players.len(), 2);
    assert!(replay.players.iter().all(|player| player.fleet.len() == 5));
    assert!(matches!(
        replay.timeline.first(),
        Some(GameTimelineEvent::Attack(record)) if record.turn_number == turn
    ));
    assert_eq!(
        room.replay_for(Uuid::new_v4()).unwrap_err(),
        GameError::NotRoomMember
    );
    let serialized = serde_json::to_string(&replay).unwrap();
    assert!(!serialized.contains("sessionId"));
}

#[test]
fn lobby_readiness_and_explicit_start_are_idempotent_authoritative_and_race_safe() {
    let host = session("Alpha");
    let guest = session("Bravo");
    let mut room = GameRoom::new(
        "ABC234".to_string(),
        "Test operation".to_string(),
        RoomVisibility::Private,
        &host,
    )
    .unwrap();
    room.join(&guest).unwrap();
    let host_player_id = room.player_for_session(host.id).unwrap().id;
    let guest_player_id = room.player_for_session(guest.id).unwrap().id;
    let host_ready_request = Uuid::new_v4();
    let (accepted, duplicate) = room
        .set_lobby_ready(host.id, host_ready_request, host_player_id, true)
        .unwrap();
    assert!(!duplicate);
    assert_eq!(accepted.player_id, host_player_id);
    assert_eq!(room.status, RoomStatus::WaitingForReady);
    let version_after_ready = room.version;
    let (replayed, duplicate) = room
        .set_lobby_ready(host.id, host_ready_request, host_player_id, true)
        .unwrap();
    assert!(duplicate);
    assert_eq!(replayed, accepted);
    assert_eq!(room.version, version_after_ready);

    room.set_lobby_ready(guest.id, Uuid::new_v4(), guest_player_id, true)
        .unwrap();
    assert_eq!(room.status, RoomStatus::ReadyToStart);
    assert!(room.game.is_none(), "both ready must not auto-start");

    let unready_request = Uuid::new_v4();
    room.set_lobby_ready(guest.id, unready_request, guest_player_id, false)
        .unwrap();
    assert_eq!(room.status, RoomStatus::WaitingForReady);
    let version_after_unready = room.version;
    let (_, duplicate) = room
        .set_lobby_ready(guest.id, unready_request, guest_player_id, false)
        .unwrap();
    assert!(duplicate);
    assert_eq!(room.version, version_after_unready);

    room.set_lobby_ready(guest.id, Uuid::new_v4(), guest_player_id, true)
        .unwrap();
    let ready_version = room.version;
    assert_eq!(
        room.start_placement(guest.id, Uuid::new_v4(), guest_player_id, ready_version,)
            .unwrap_err(),
        GameError::NotHost
    );
    assert_eq!(
        room.start_placement(host.id, Uuid::new_v4(), host_player_id, ready_version - 1,)
            .unwrap_err(),
        GameError::StaleRoomVersion
    );

    room.disconnect(guest.id, 90).unwrap();
    assert_eq!(
        room.start_placement(host.id, Uuid::new_v4(), host_player_id, room.version)
            .unwrap_err(),
        GameError::PlayerDisconnected
    );
    room.reconnect(guest.id).unwrap();

    let start_request = Uuid::new_v4();
    let (started, duplicate) = room
        .start_placement(host.id, start_request, host_player_id, room.version)
        .unwrap();
    assert!(!duplicate);
    assert_eq!(room.status, RoomStatus::Placement);
    let started_version = room.version;
    let (replayed, duplicate) = room
        .start_placement(host.id, start_request, host_player_id, ready_version)
        .unwrap();
    assert!(duplicate);
    assert_eq!(replayed, started);
    assert_eq!(room.version, started_version);
    assert_eq!(
        room.start_placement(host.id, Uuid::new_v4(), host_player_id, room.version)
            .unwrap_err(),
        GameError::GameAlreadyStarted
    );
    assert_eq!(
        room.set_lobby_ready(host.id, Uuid::new_v4(), host_player_id, false)
            .unwrap_err(),
        GameError::GameAlreadyStarted
    );
}

#[test]
fn confirmation_rejects_a_placement_that_differs_from_server_state() {
    let host = session("Alpha");
    let guest = session("Bravo");
    let mut room = GameRoom::new(
        "ABC234".to_string(),
        "Test operation".to_string(),
        RoomVisibility::Private,
        &host,
    )
    .unwrap();
    room.join(&guest).unwrap();
    prepare_placement(&mut room, &host, &guest);
    room.place_ships(host.id, fleet(0)).unwrap();
    assert_eq!(
        room.confirm_placement(host.id, &fleet(5), 60).unwrap_err(),
        GameError::PlacementMismatch
    );
}

#[test]
fn typed_chat_is_validated_idempotent_rate_limited_and_room_scoped() {
    let host = session("Alpha");
    let other_host = session("Charlie");
    let mut room = GameRoom::new(
        "ABC234".to_string(),
        "Test operation".to_string(),
        RoomVisibility::Private,
        &host,
    )
    .unwrap();
    let other_room = GameRoom::new(
        "XYZ234".to_string(),
        "Other operation".to_string(),
        RoomVisibility::Private,
        &other_host,
    )
    .unwrap();
    let now = Utc::now();
    assert_eq!(
        room.send_chat_at(
            other_host.id,
            Uuid::new_v4(),
            ChatMessageType::Text,
            Some("intrusion".to_string()),
            None,
            now,
        )
        .unwrap_err(),
        GameError::NotRoomMember
    );
    assert_eq!(
        room.chat_history(other_host.id).unwrap_err(),
        GameError::NotRoomMember
    );
    let message_id = Uuid::new_v4();
    let (message, duplicate) = room
        .send_chat_at(
            host.id,
            message_id,
            ChatMessageType::Text,
            Some("  ready\nfor battle  ".to_string()),
            None,
            now,
        )
        .unwrap();
    assert!(!duplicate);
    assert_eq!(message.content, "ready\nfor battle");
    assert_eq!(message.room_id, room.id);
    assert_ne!(message.room_id, other_room.id);
    assert_eq!(message.player_id, Some(room.players[0].id));
    assert_eq!(
        room.send_chat_at(
            host.id,
            Uuid::new_v4(),
            ChatMessageType::Text,
            Some("   ".to_string()),
            None,
            now + Duration::seconds(1),
        )
        .unwrap_err(),
        GameError::InvalidChatMessage
    );
    assert_eq!(
        room.send_chat_at(
            host.id,
            Uuid::new_v4(),
            ChatMessageType::Text,
            Some("<script>alert(1)</script>".to_string()),
            None,
            now + Duration::seconds(1),
        )
        .unwrap_err(),
        GameError::InvalidChatMessage
    );
    assert_eq!(
        room.send_chat_at(
            host.id,
            Uuid::new_v4(),
            ChatMessageType::Text,
            Some("x".repeat(301)),
            None,
            now + Duration::seconds(1),
        )
        .unwrap_err(),
        GameError::InvalidChatMessage
    );
    let (emoji, _) = room
        .send_chat_at(
            host.id,
            Uuid::new_v4(),
            ChatMessageType::Emoji,
            Some("🎯".to_string()),
            None,
            now + Duration::seconds(1),
        )
        .unwrap();
    assert_eq!(emoji.message_type, ChatMessageType::Emoji);
    assert_eq!(emoji.content, "🎯");
    assert_eq!(
        room.send_chat_at(
            host.id,
            Uuid::new_v4(),
            ChatMessageType::Emoji,
            Some("<img>".to_string()),
            None,
            now + Duration::seconds(2),
        )
        .unwrap_err(),
        GameError::InvalidEmoji
    );
    let (quick, _) = room
        .send_chat_at(
            host.id,
            Uuid::new_v4(),
            ChatMessageType::QuickCommand,
            None,
            Some(QuickCommandId::NiceShot),
            now + Duration::seconds(3),
        )
        .unwrap();
    assert_eq!(quick.content, "나이스 샷");
    assert_eq!(quick.command_id, Some(QuickCommandId::NiceShot));
    assert_eq!(
        room.send_chat_at(
            host.id,
            Uuid::new_v4(),
            ChatMessageType::QuickCommand,
            None,
            Some(QuickCommandId::NiceShot),
            now + Duration::seconds(4),
        )
        .unwrap_err(),
        GameError::RateLimited
    );

    let before = room.chat_messages.len();
    let (replayed, duplicate) = room
        .send_chat_at(
            host.id,
            message_id,
            ChatMessageType::Text,
            Some("changed".to_string()),
            None,
            now + Duration::seconds(5),
        )
        .unwrap();
    assert!(duplicate);
    assert_eq!(replayed.content, "ready\nfor battle");
    assert_eq!(room.chat_messages.len(), before);

    let spammer = session("Delta");
    let mut spam_room = GameRoom::new(
        "SPM234".to_string(),
        "Spam operation".to_string(),
        RoomVisibility::Private,
        &spammer,
    )
    .unwrap();
    for index in 0..3 {
        spam_room
            .send_chat_at(
                spammer.id,
                Uuid::new_v4(),
                ChatMessageType::Text,
                Some(format!("message {index}")),
                None,
                now,
            )
            .unwrap();
    }
    assert_eq!(
        spam_room
            .send_chat_at(
                spammer.id,
                Uuid::new_v4(),
                ChatMessageType::Emoji,
                Some("🔥".to_string()),
                None,
                now,
            )
            .unwrap_err(),
        GameError::RateLimited
    );

    for index in 0..110 {
        room.push_system_message(format!("system event {index}"));
    }
    assert_eq!(room.chat_messages.len(), MAX_CHAT_HISTORY);
    assert!(
        room.chat_history(host.id)
            .unwrap()
            .iter()
            .all(|entry| entry.room_id == room.id)
    );
}

#[test]
fn turn_expiry_changes_turn_resets_on_attack_and_forfeits_after_three() {
    let (mut room, first, second) = playing_room();
    let timed_out_player = room.game.as_ref().unwrap().current_player_id;
    let timed_out_session = if room.player_for_session(first.id).unwrap().id == timed_out_player {
        first.id
    } else {
        second.id
    };
    let opponent_session = if timed_out_session == first.id {
        second.id
    } else {
        first.id
    };

    for cycle in 0..3 {
        let game = room.game.as_ref().unwrap();
        let deadline = game.turn_deadline_at.unwrap();
        let turn = game.turn_number;
        let record = room
            .expire_turn(turn, timed_out_player, deadline, deadline)
            .unwrap()
            .unwrap();
        assert_eq!(record.consecutive_timeout_count, cycle + 1);
        if cycle == 2 {
            assert!(record.winner_id.is_some());
            break;
        }
        let opponent_id = room.player_for_session(opponent_session).unwrap().id;
        let turn = room.game.as_ref().unwrap().turn_number;
        room.fire(
            opponent_session,
            Uuid::new_v4(),
            opponent_id,
            Coordinate {
                row: 9,
                col: 9 - cycle,
            },
            room.version,
            turn,
        )
        .unwrap();
    }
    let result = room.game.as_ref().unwrap().result.as_ref().unwrap();
    assert_eq!(result.finish_reason, FinishReason::TurnTimeout);
    assert_eq!(result.win_type, crate::domain::WinType::Timeout);
    assert_eq!(room.status, RoomStatus::Finished);
    assert_eq!(
        result
            .players
            .iter()
            .find(|stats| stats.player_id == timed_out_player)
            .unwrap()
            .total_timeouts,
        3
    );
}

#[test]
fn stale_expiry_cannot_override_a_normal_attack() {
    let (mut room, first, second) = playing_room();
    let current = room.game.as_ref().unwrap().current_player_id;
    let session_id = if room.player_for_session(first.id).unwrap().id == current {
        first.id
    } else {
        second.id
    };
    let old_turn = room.game.as_ref().unwrap().turn_number;
    let old_deadline = room.game.as_ref().unwrap().turn_deadline_at.unwrap();
    room.game.as_mut().unwrap().turn_deadline_at = Some(old_deadline + Duration::seconds(60));
    room.fire(
        session_id,
        Uuid::new_v4(),
        current,
        Coordinate { row: 9, col: 9 },
        room.version,
        old_turn,
    )
    .unwrap();
    assert!(
        room.expire_turn(old_turn, current, old_deadline, old_deadline)
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_normal_attack_resets_the_attacking_players_consecutive_timeouts() {
    let (mut room, first, second) = playing_room();
    let timed_out_player = room.game.as_ref().unwrap().current_player_id;
    let timed_out_session = if room.player_for_session(first.id).unwrap().id == timed_out_player {
        first.id
    } else {
        second.id
    };
    let opponent_session = if timed_out_session == first.id {
        second.id
    } else {
        first.id
    };

    let game = room.game.as_ref().unwrap();
    let deadline = game.turn_deadline_at.unwrap();
    room.expire_turn(game.turn_number, timed_out_player, deadline, deadline)
        .unwrap()
        .unwrap();

    let opponent_id = room.player_for_session(opponent_session).unwrap().id;
    let turn = room.game.as_ref().unwrap().turn_number;
    room.fire(
        opponent_session,
        Uuid::new_v4(),
        opponent_id,
        Coordinate { row: 9, col: 9 },
        room.version,
        turn,
    )
    .unwrap();

    let turn = room.game.as_ref().unwrap().turn_number;
    room.fire(
        timed_out_session,
        Uuid::new_v4(),
        timed_out_player,
        Coordinate { row: 9, col: 8 },
        room.version,
        turn,
    )
    .unwrap();

    assert_eq!(
        room.game
            .as_ref()
            .unwrap()
            .consecutive_timeout_counts
            .get(&timed_out_player),
        Some(&0)
    );
    assert_eq!(
        room.game
            .as_ref()
            .unwrap()
            .total_timeout_counts
            .get(&timed_out_player),
        Some(&1)
    );
}

#[test]
fn internal_room_state_round_trips_after_attacks() {
    let (mut room, first, second) = playing_room();
    let current_id = room.game.as_ref().unwrap().current_player_id;
    let session_id = if room.player_for_session(first.id).unwrap().id == current_id {
        first.id
    } else {
        second.id
    };
    let version = room.version;
    room.fire(
        session_id,
        Uuid::new_v4(),
        current_id,
        Coordinate { row: 9, col: 9 },
        version,
        1,
    )
    .unwrap();
    let json = serde_json::to_string(&room).unwrap();
    let restored: GameRoom = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.game.unwrap().attacks.len(), 1);
    assert_eq!(restored.chat_messages, room.chat_messages);
}

#[test]
fn pre_catalog_room_snapshots_are_fixed_to_v1_instead_of_current_at_read_time() {
    let (room, _, _) = playing_room();
    let mut value = serde_json::to_value(room).unwrap();
    value.as_object_mut().unwrap().remove("balance");
    value
        .get_mut("game")
        .and_then(serde_json::Value::as_object_mut)
        .unwrap()
        .remove("balance");

    let restored: GameRoom = serde_json::from_value(value).unwrap();
    assert_eq!(restored.balance, BalancePin::v1());
    assert_eq!(restored.game.as_ref().unwrap().balance, BalancePin::v1());
    assert!(restored.has_valid_balance_pin());
}

#[test]
fn a_tampered_balance_pin_cannot_be_snapshotted_or_executed() {
    let (mut room, first, _) = playing_room();
    room.balance.manifest.rapid_turn_duration_seconds = 20;
    assert!(!room.has_valid_balance_pin());
    assert_eq!(
        room.snapshot_for(first.id).unwrap_err(),
        GameError::InvalidState
    );
}
