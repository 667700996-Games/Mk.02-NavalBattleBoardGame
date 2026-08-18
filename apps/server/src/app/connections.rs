use super::*;

#[derive(Debug, Clone)]
struct ConnectionEntry {
    connection_id: Uuid,
    protocol_version: u16,
    sender: mpsc::Sender<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ConnectionHub {
    connections: Arc<DashMap<Uuid, ConnectionEntry>>,
}

impl ConnectionHub {
    pub fn connect(
        &self,
        session_id: Uuid,
        protocol_version: u16,
        sender: mpsc::Sender<String>,
    ) -> Uuid {
        let connection_id = Uuid::new_v4();
        self.connections.insert(
            session_id,
            ConnectionEntry {
                connection_id,
                protocol_version,
                sender,
            },
        );
        connection_id
    }

    pub fn disconnect_if_current(&self, session_id: Uuid, connection_id: Uuid) -> bool {
        let is_current = self
            .connections
            .get(&session_id)
            .map(|entry| entry.connection_id == connection_id)
            .unwrap_or(false);
        if is_current {
            self.connections.remove(&session_id);
        }
        is_current
    }

    pub fn send(&self, session_id: Uuid, event: ServerEvent) {
        let Ok(serialized) = serde_json::to_string(&event) else {
            tracing::error!(%session_id, "server event serialization failed");
            return;
        };
        self.send_serialized(session_id, serialized);
    }

    pub fn send_serialized(&self, session_id: Uuid, serialized: String) {
        let Some(connection) = self.connections.get(&session_id) else {
            return;
        };
        let connection_id = connection.connection_id;
        let send_result = connection.sender.try_send(serialized);
        drop(connection);
        if let Err(error) = send_result {
            let reason = match error {
                mpsc::error::TrySendError::Full(_) => "websocket slow consumer disconnected",
                mpsc::error::TrySendError::Closed(_) => "websocket closed consumer removed",
            };
            if self.disconnect_if_current(session_id, connection_id) {
                tracing::warn!(%session_id, %connection_id, %reason);
            }
        }
    }

    pub fn close(&self, session_id: Uuid) -> bool {
        self.connections.remove(&session_id).is_some()
    }

    pub fn protocol_version(&self, session_id: Uuid) -> Option<u16> {
        self.connections
            .get(&session_id)
            .map(|entry| entry.protocol_version)
    }

    pub fn len(&self) -> usize {
        self.connections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }
}
