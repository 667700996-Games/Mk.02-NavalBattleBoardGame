use super::*;

impl GameRoom {
    pub(super) fn refresh_lobby_status(&mut self) {
        self.status = if self.players.len() < 2 {
            RoomStatus::WaitingForOpponent
        } else if self
            .players
            .iter()
            .all(|player| player.ready_state == PlayerReadyState::Ready)
        {
            RoomStatus::ReadyToStart
        } else {
            RoomStatus::WaitingForReady
        };
    }

    pub(super) fn reset_lobby_after_guest_departure(&mut self) {
        self.pending_placements.clear();
        self.game = None;
        self.game_id = None;
        self.placement_started_at = None;
        self.ready_resolutions.clear();
        self.start_resolutions.clear();
        self.disconnected_deadlines.clear();
        for player in &mut self.players {
            player.ready_state = PlayerReadyState::NotReady;
            player.ready_at = None;
            player.placement_confirmed = false;
        }
        self.status = RoomStatus::WaitingForOpponent;
    }

    pub(super) fn remember_ready_resolution(&mut self, record: PlayerReadyRecord) {
        if self.ready_resolutions.len() >= 128 {
            if let Some(oldest) = self
                .ready_resolutions
                .iter()
                .min_by_key(|(_, resolution)| resolution.accepted_at)
                .map(|(request_id, _)| *request_id)
            {
                self.ready_resolutions.remove(&oldest);
            }
        }
        self.ready_resolutions.insert(record.request_id, record);
    }

    pub(super) fn remember_start_resolution(&mut self, record: GameStartRecord) {
        if self.start_resolutions.len() >= 64 {
            if let Some(oldest) = self
                .start_resolutions
                .iter()
                .min_by_key(|(_, resolution)| resolution.started_at)
                .map(|(request_id, _)| *request_id)
            {
                self.start_resolutions.remove(&oldest);
            }
        }
        self.start_resolutions.insert(record.request_id, record);
    }

    pub fn can_start_game(&self) -> bool {
        self.status == RoomStatus::ReadyToStart
            && self.game_id.is_none()
            && self.players.len() == 2
            && self
                .players
                .iter()
                .all(|player| player.ready_state == PlayerReadyState::Ready)
            && self
                .players
                .iter()
                .all(|player| player.connection_state == ConnectionState::Online)
    }

    pub(super) fn bump(&mut self) {
        self.version += 1;
        self.updated_at = Utc::now();
    }
}
