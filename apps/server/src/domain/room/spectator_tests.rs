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

fn public_playing_room() -> (GameRoom, UserSession, UserSession) {
    let first = session("Alpha");
    let second = session("Bravo");
    let mut room = GameRoom::new(
        "WATCH1".to_string(),
        "Public watch exercise".to_string(),
        RoomVisibility::Public,
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
    assert!(!room.confirm_placement(first.id, &fleet(0), 60).unwrap());
    assert!(room.confirm_placement(second.id, &fleet(5), 60).unwrap());
    (room, first, second)
}

#[test]
fn projection_is_delayed_public_and_never_contains_hidden_fleets() {
    let (mut room, first, second) = public_playing_room();
    let active_player_id = room.game.as_ref().unwrap().current_player_id;
    let active_session_id = if room.player_for_session(first.id).unwrap().id == active_player_id {
        first.id
    } else {
        second.id
    };
    let turn = room.game.as_ref().unwrap().turn_number;
    let (attack, _) = room
        .fire(
            active_session_id,
            Uuid::new_v4(),
            active_player_id,
            Coordinate { row: 9, col: 9 },
            room.version,
            turn,
        )
        .unwrap();

    let before_delay = room
        .spectator_snapshot_at(attack.created_at + Duration::seconds(29))
        .unwrap();
    assert!(before_delay.timeline.is_empty());
    assert_eq!(before_delay.delay_seconds, 30);
    assert!(before_delay.result.is_none());

    let after_delay = room
        .spectator_snapshot_at(attack.created_at + Duration::seconds(30))
        .unwrap();
    assert_eq!(after_delay.timeline.len(), 1);
    assert_eq!(after_delay.phase, SpectatorPhase::Live);
    let serialized = serde_json::to_string(&after_delay).unwrap();
    for hidden_field in [
        "\"boards\"",
        "\"ships\"",
        "sessionId",
        "pendingPlacements",
        "placement",
    ] {
        assert!(
            !serialized.contains(hidden_field),
            "spectator payload leaked {hidden_field}"
        );
    }

    room.visibility = RoomVisibility::Private;
    assert_eq!(
        room.spectator_snapshot_at(attack.created_at + Duration::seconds(30))
            .unwrap_err(),
        GameError::RoomNotFound
    );
    room.visibility = RoomVisibility::Public;

    let surrendering_player_id = room.game.as_ref().unwrap().current_player_id;
    let surrendering_session_id =
        if room.player_for_session(first.id).unwrap().id == surrendering_player_id {
            first.id
        } else {
            second.id
        };
    room.surrender(surrendering_session_id, surrendering_player_id)
        .unwrap();
    let game = room.game.as_ref().unwrap();
    let finished_at = game.result.as_ref().unwrap().finished_at;
    assert_eq!(
        room.spectator_snapshot_at(game.started_at + Duration::seconds(30))
            .unwrap_err(),
        GameError::RoomNotFound
    );
    assert_eq!(
        room.spectator_snapshot_at(finished_at + Duration::seconds(30))
            .unwrap_err(),
        GameError::RoomNotFound
    );
}
