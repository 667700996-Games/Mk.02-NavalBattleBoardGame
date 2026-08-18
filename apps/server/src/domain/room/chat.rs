use super::*;

impl GameRoom {
    pub fn send_chat(
        &mut self,
        session_id: Uuid,
        client_message_id: Uuid,
        message_type: ChatMessageType,
        content: Option<String>,
        command_id: Option<QuickCommandId>,
    ) -> Result<(ChatMessage, bool), GameError> {
        self.send_chat_at(
            session_id,
            client_message_id,
            message_type,
            content,
            command_id,
            Utc::now(),
        )
    }

    pub(super) fn send_chat_at(
        &mut self,
        session_id: Uuid,
        client_message_id: Uuid,
        message_type: ChatMessageType,
        content: Option<String>,
        command_id: Option<QuickCommandId>,
        now: DateTime<Utc>,
    ) -> Result<(ChatMessage, bool), GameError> {
        if !matches!(
            self.status,
            RoomStatus::WaitingForOpponent
                | RoomStatus::WaitingForReady
                | RoomStatus::ReadyToStart
                | RoomStatus::Placement
                | RoomStatus::Playing
                | RoomStatus::Finished
        ) {
            return Err(GameError::InvalidState);
        }
        let player = self.player_for_session(session_id)?.clone();
        if let Some(previous) = self
            .chat_messages
            .iter()
            .find(|message| message.message_id == client_message_id)
        {
            if previous.player_id == Some(player.id) {
                return Ok((previous.clone(), true));
            }
            return Err(GameError::Unauthorized);
        }
        let (normalized, resolved_command) =
            match message_type {
                ChatMessageType::Text => (
                    normalize_chat_message(content.ok_or(GameError::InvalidChatMessage)?)?,
                    None,
                ),
                ChatMessageType::Emoji => {
                    let emoji = content.ok_or(GameError::InvalidEmoji)?;
                    if command_id.is_some() || !ALLOWED_EMOJIS.contains(&emoji.as_str()) {
                        return Err(GameError::InvalidEmoji);
                    }
                    (emoji, None)
                }
                ChatMessageType::QuickCommand => {
                    if content.is_some() {
                        return Err(GameError::InvalidQuickCommand);
                    }
                    let command = command_id.ok_or(GameError::InvalidQuickCommand)?;
                    if self.last_quick_commands.get(&player.id).is_some_and(
                        |(previous, sent_at)| {
                            *previous == command
                                && now.signed_duration_since(*sent_at).num_milliseconds() < 2_000
                        },
                    ) {
                        return Err(GameError::RateLimited);
                    }
                    (command.label().to_string(), Some(command))
                }
                ChatMessageType::System => return Err(GameError::InvalidChatMessage),
            };
        if self
            .chat_blocked_until
            .get(&player.id)
            .is_some_and(|blocked_until| *blocked_until > now)
        {
            return Err(GameError::RateLimited);
        }
        let window = self.chat_rate_windows.entry(player.id).or_default();
        window.retain(|sent_at| now.signed_duration_since(*sent_at).num_seconds() < 10);
        let recent_two_seconds = window
            .iter()
            .filter(|sent_at| now.signed_duration_since(**sent_at).num_milliseconds() < 2_000)
            .count();
        if window.len() >= 8 || recent_two_seconds >= 3 {
            self.chat_blocked_until
                .insert(player.id, now + Duration::seconds(3));
            return Err(GameError::RateLimited);
        }
        window.push(now);
        if let Some(command) = resolved_command {
            self.last_quick_commands.insert(player.id, (command, now));
        }
        let message = ChatMessage {
            message_id: client_message_id,
            room_id: self.id,
            player_id: Some(player.id),
            nickname: player.nickname,
            content: normalized,
            timestamp: now,
            message_type,
            command_id: resolved_command,
        };
        self.append_chat_message(message.clone());
        Ok((message, false))
    }

    pub fn chat_history(&self, session_id: Uuid) -> Result<Vec<ChatMessage>, GameError> {
        self.player_for_session(session_id)?;
        Ok(self.chat_messages.clone())
    }

    pub fn record_start_rejection(
        &mut self,
        session_id: Uuid,
        error_code: &str,
    ) -> Result<ChatMessage, GameError> {
        let player = self.player_for_session(session_id)?;
        let nickname = player.nickname.clone();
        Ok(self.push_system_message(format!(
            "{} 지휘관의 게임 시작 요청이 거부되었습니다. ({})",
            nickname, error_code
        )))
    }

    pub fn typing_event(
        &self,
        session_id: Uuid,
        is_typing: bool,
    ) -> Result<ChatTypingEvent, GameError> {
        if self.status == RoomStatus::Cancelled {
            return Err(GameError::InvalidState);
        }
        let player = self.player_for_session(session_id)?;
        Ok(ChatTypingEvent {
            room_id: self.id,
            player_id: player.id,
            nickname: player.nickname.clone(),
            is_typing,
        })
    }

    pub(super) fn push_system_message(&mut self, message: impl Into<String>) -> ChatMessage {
        let message = ChatMessage {
            message_id: Uuid::new_v4(),
            room_id: self.id,
            player_id: None,
            nickname: "SYSTEM".to_string(),
            content: message.into(),
            timestamp: Utc::now(),
            message_type: ChatMessageType::System,
            command_id: None,
        };
        self.append_chat_message(message.clone());
        message
    }

    pub(super) fn append_chat_message(&mut self, message: ChatMessage) {
        self.chat_messages.push(message);
        if self.chat_messages.len() > MAX_CHAT_HISTORY {
            let excess = self.chat_messages.len() - MAX_CHAT_HISTORY;
            self.chat_messages.drain(..excess);
        }
        self.updated_at = Utc::now();
    }
}

fn normalize_chat_message(message: String) -> Result<String, GameError> {
    let normalized = message.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim();
    let count = trimmed.chars().count();
    let safe = (1..=MAX_CHAT_MESSAGE_CHARS).contains(&count)
        && !trimmed.contains(['<', '>'])
        && trimmed
            .chars()
            .all(|character| !character.is_control() || character == '\n');
    if safe {
        Ok(trimmed.to_string())
    } else {
        Err(GameError::InvalidChatMessage)
    }
}
