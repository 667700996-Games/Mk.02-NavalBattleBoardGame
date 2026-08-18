use super::*;

impl AppState {
    pub async fn create_session(
        &self,
        nickname: String,
    ) -> Result<(UserSession, String), GameError> {
        let nickname = nickname.trim().to_string();
        validate_nickname(&nickname)?;
        let token = random_token();
        let token_hash = hash_token(&token);
        let now = Utc::now();
        let session = UserSession {
            id: Uuid::new_v4(),
            account_id: None,
            nickname,
            token_hash,
            created_at: now,
            last_seen_at: now,
            current_room_id: None,
        };
        self.store.save_session(&session).await?;
        Ok((session, token))
    }

    pub async fn upgrade_account(
        &self,
        session: &UserSession,
        handle: String,
    ) -> Result<(PlayerAccount, String, String), GameError> {
        if session.account_id.is_some() {
            return Err(GameError::InvalidState);
        }
        if session.current_room_id.is_some() {
            return Err(GameError::InvalidState);
        }
        let handle = handle.trim().to_string();
        validate_nickname(&handle)?;
        let recovery_key = random_token();
        let next_session_token = random_token();
        let account = PlayerAccount {
            id: Uuid::new_v4(),
            handle,
            created_at: Utc::now(),
        };
        self.store
            .create_account(
                session.id,
                &account,
                &hash_token(&recovery_key),
                &hash_token(&next_session_token),
            )
            .await?;
        Ok((account, recovery_key, next_session_token))
    }

    pub async fn login_account(
        &self,
        account_id: Uuid,
        recovery_key: String,
    ) -> Result<(UserSession, String), GameError> {
        if recovery_key.len() != 43
            || !recovery_key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(GameError::Unauthorized);
        }
        let account = self
            .store
            .account_by_credentials(account_id, &hash_token(&recovery_key))
            .await?
            .ok_or(GameError::Unauthorized)?;
        match self.store.active_penalty(account.id, account.id).await? {
            Some(ActivePenalty::Banned) => return Err(GameError::AccountBanned),
            Some(ActivePenalty::Suspended(_)) => return Err(GameError::AccountSuspended),
            None => {}
        }
        let token = random_token();
        let now = Utc::now();
        let session = UserSession {
            id: Uuid::new_v4(),
            account_id: Some(account.id),
            nickname: account.handle,
            token_hash: hash_token(&token),
            created_at: now,
            last_seen_at: now,
            current_room_id: None,
        };
        self.store.save_session(&session).await?;
        Ok((session, token))
    }

    pub async fn account_sessions(
        &self,
        session: &UserSession,
    ) -> Result<Vec<AccountSession>, GameError> {
        self.store
            .sessions_for_account(session.account_id.ok_or(GameError::Unauthorized)?)
            .await
    }

    pub async fn export_account_data(
        &self,
        session: &UserSession,
    ) -> Result<serde_json::Value, GameError> {
        let account_id = session.account_id.ok_or(GameError::Unauthorized)?;
        let request_id = Uuid::new_v4();
        let generated_at = Utc::now();
        let subject_fingerprint = hash_token(&format!("{account_id}:{request_id}"));
        self.store
            .export_account_data(account_id, request_id, &subject_fingerprint, generated_at)
            .await
    }

    pub async fn delete_account(
        &self,
        session: &UserSession,
        recovery_key: String,
        confirmation: String,
    ) -> Result<(Uuid, chrono::DateTime<Utc>, AccountDeletionStats), GameError> {
        let account_id = session.account_id.ok_or(GameError::Unauthorized)?;
        if confirmation != "DELETE"
            || recovery_key.len() != 43
            || !recovery_key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(GameError::InvalidRequest);
        }
        self.store
            .account_by_credentials(account_id, &hash_token(&recovery_key))
            .await?
            .ok_or(GameError::Unauthorized)?;

        let account_sessions = self.store.sessions_for_account(account_id).await?;
        let session_ids: HashSet<_> = account_sessions.iter().map(|item| item.id).collect();
        let mut known_room_ids: HashSet<_> = account_sessions
            .iter()
            .filter_map(|item| item.current_room_id)
            .collect();
        let cached_rooms: Vec<_> = self
            .rooms
            .iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect();
        for (room_id, room) in cached_rooms {
            if room
                .lock()
                .await
                .players
                .iter()
                .any(|player| session_ids.contains(&player.session_id))
            {
                known_room_ids.insert(room_id);
            }
        }

        for account_session in &account_sessions {
            let _ = self.cancel_matchmaking(account_session.id).await;
            if let Some(room_id) = account_session.current_room_id {
                let session_to_remove = UserSession {
                    id: account_session.id,
                    account_id: Some(account_id),
                    nickname: account_session.nickname.clone(),
                    token_hash: String::new(),
                    created_at: account_session.created_at,
                    last_seen_at: account_session.last_seen_at,
                    current_room_id: Some(room_id),
                };
                match self.leave_room(&session_to_remove, room_id).await {
                    Ok(room) => {
                        self.broadcast_snapshots(&room, SnapshotEvent::PlayerLeft)
                            .await;
                        self.broadcast_latest_chat_message(&room).await;
                    }
                    Err(GameError::RoomNotFound) => {
                        self.store
                            .update_session_room(account_session.id, None)
                            .await?;
                    }
                    Err(error) => return Err(error),
                }
            }
            self.close_session_everywhere(account_session.id, GameError::Unauthorized)
                .await;
        }

        let request_id = Uuid::new_v4();
        let deleted_at = Utc::now();
        let subject_fingerprint = hash_token(&format!("{account_id}:{request_id}"));
        let known_room_ids: Vec<_> = known_room_ids.into_iter().collect();
        let stats = self
            .store
            .delete_account_data(
                account_id,
                request_id,
                &subject_fingerprint,
                &known_room_ids,
                deleted_at,
                AccountDeletionScope::LiveRequest,
            )
            .await?;
        for room_id in known_room_ids {
            self.rooms.remove(&room_id);
            self.cancel_turn_expiry(room_id);
        }
        Ok((request_id, deleted_at, stats))
    }

    pub async fn progression(&self, session: &UserSession) -> Result<PlayerProgression, GameError> {
        let history = self.store.history_for_session(session.id).await?;
        let rewards = match session.account_id {
            Some(account_id) => self.store.mission_rewards(account_id).await?,
            None => Vec::new(),
        };
        let now = Utc::now();
        let live_content = self.active_live_content(now).await?;
        let ranked = match session.account_id {
            Some(account_id) => Some(
                self.store
                    .ranked_profile(
                        account_id,
                        &live_content.season.id,
                        live_content.season.starts_at,
                        now,
                    )
                    .await?,
            ),
            None => None,
        };
        Ok(build_progression(
            session,
            &history,
            &rewards,
            ranked,
            &live_content,
            now,
        ))
    }

    pub async fn ranked_leaderboard(
        &self,
        session: &UserSession,
        requested_season_id: Option<&str>,
        cursor: Option<Uuid>,
        limit: Option<u32>,
    ) -> Result<(RankedLeaderboardPage, bool), GameError> {
        let account_id = session.account_id.ok_or(GameError::RankedAccountRequired)?;
        let now = Utc::now();
        let live_content = self.active_live_content(now).await?;
        let content_history = self.store.live_content_history(100).await?;
        let season_id = requested_season_id.unwrap_or(&live_content.season.id);
        if !(3..=32).contains(&season_id.len())
            || !season_id.chars().all(|character| {
                character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
            })
        {
            return Err(GameError::InvalidRequest);
        }
        let season_ends_at = if season_id == live_content.season.id {
            Some(live_content.season.ends_at)
        } else {
            content_history
                .iter()
                .find(|revision| revision.season.id == season_id)
                .map(|revision| revision.season.ends_at)
        };
        let archived = season_ends_at.map_or(season_id != live_content.season.id, |ends_at| {
            now >= ends_at + chrono::Duration::hours(RANKED_LEADERBOARD_FINALIZATION_HOURS)
        });
        let limit = usize::try_from(limit.unwrap_or(RANKED_LEADERBOARD_DEFAULT_LIMIT as u32))
            .map_err(|_| GameError::InvalidRequest)?;
        if limit == 0 || limit > RANKED_LEADERBOARD_MAX_LIMIT {
            return Err(GameError::InvalidRequest);
        }
        let mut page = self
            .store
            .ranked_leaderboard(
                season_id,
                &live_content.season.id,
                archived,
                cursor,
                limit,
                now,
            )
            .await?;
        for season in &mut page.available_seasons {
            let ends_at = if season.season_id == live_content.season.id {
                Some(live_content.season.ends_at)
            } else {
                content_history
                    .iter()
                    .find(|revision| revision.season.id == season.season_id)
                    .map(|revision| revision.season.ends_at)
            };
            season.archived =
                ends_at.map_or(season.season_id != live_content.season.id, |ends_at| {
                    now >= ends_at + chrono::Duration::hours(RANKED_LEADERBOARD_FINALIZATION_HOURS)
                });
        }
        if season_id != live_content.season.id
            && !page
                .available_seasons
                .iter()
                .any(|season| season.season_id == season_id)
        {
            return Err(GameError::InvalidRequest);
        }
        let viewer_visible = self.store.ranked_leaderboard_visibility(account_id).await?;
        self.metrics
            .ranked_leaderboard_requests
            .fetch_add(1, Ordering::Relaxed);
        self.metrics
            .ranked_leaderboard_entries_served
            .fetch_add(page.entries.len() as u64, Ordering::Relaxed);
        if page.entries.is_empty() {
            self.metrics
                .ranked_leaderboard_empty_responses
                .fetch_add(1, Ordering::Relaxed);
        }
        Ok((page, viewer_visible))
    }

    pub async fn set_ranked_leaderboard_visibility(
        &self,
        session: &UserSession,
        visible: bool,
    ) -> Result<bool, GameError> {
        let account_id = session.account_id.ok_or(GameError::RankedAccountRequired)?;
        self.store
            .set_ranked_leaderboard_visibility(account_id, visible)
            .await?;
        self.metrics
            .ranked_leaderboard_visibility_changes
            .fetch_add(1, Ordering::Relaxed);
        Ok(visible)
    }

    pub async fn claim_mission_reward(
        &self,
        session: &UserSession,
        mission_id: &str,
    ) -> Result<PlayerProgression, GameError> {
        let account_id = session.account_id.ok_or(GameError::Unauthorized)?;
        let history = self.store.history_for_session(session.id).await?;
        let rewards = self.store.mission_rewards(account_id).await?;
        let now = Utc::now();
        let live_content = self.active_live_content(now).await?;
        let ranked = Some(
            self.store
                .ranked_profile(
                    account_id,
                    &live_content.season.id,
                    live_content.season.starts_at,
                    now,
                )
                .await?,
        );
        let progression = build_progression(
            session,
            &history,
            &rewards,
            ranked.clone(),
            &live_content,
            now,
        );
        let mission = progression
            .missions
            .iter()
            .find(|mission| mission.id == mission_id)
            .ok_or(GameError::InvalidRequest)?;
        if !mission.completed {
            return Err(GameError::InvalidState);
        }
        let period_key = mission_period_key(mission.cadence, now);
        self.store
            .claim_mission_reward(account_id, mission.id, &period_key, mission.reward_xp)
            .await?;
        let rewards = self.store.mission_rewards(account_id).await?;
        Ok(build_progression(
            session,
            &history,
            &rewards,
            ranked,
            &live_content,
            now,
        ))
    }
}

pub(super) fn build_progression(
    session: &UserSession,
    history: &[GameHistoryItem],
    rewards: &[MissionReward],
    ranked: Option<RankedProfile>,
    live_content: &LiveContentRevision,
    now: chrono::DateTime<Utc>,
) -> PlayerProgression {
    let mut wins = 0_u32;
    let mut shots = 0_u32;
    let mut hits = 0_u32;
    let mut ships_sunk = 0_u32;
    let mut daily_games = 0_u32;
    let mut daily_hits = 0_u32;
    let mut weekly_wins = 0_u32;
    for item in history {
        let won = item.result.winner_id == item.self_player_id;
        wins += u32::from(won);
        let player = item
            .result
            .players
            .iter()
            .find(|player| player.player_id == item.self_player_id);
        if let Some(player) = player {
            shots = shots.saturating_add(player.shots);
            hits = hits.saturating_add(player.hits);
            ships_sunk = ships_sunk.saturating_add(u32::from(player.ships_sunk));
        }
        let finished_date = item.result.finished_at.date_naive();
        if finished_date == now.date_naive() {
            daily_games += 1;
            daily_hits = daily_hits.saturating_add(player.map_or(0, |stats| stats.hits));
        }
        if finished_date.iso_week() == now.date_naive().iso_week() {
            weekly_wins += u32::from(won);
        }
    }
    let games_played = u32::try_from(history.len()).unwrap_or(u32::MAX);
    let losses = games_played.saturating_sub(wins);
    // Progression is a deterministic projection of the authoritative result ledger. Re-saving a
    // result cannot double-award XP, and correcting/removing a result automatically rolls it back.
    let result_xp = u64::from(games_played) * 100
        + u64::from(wins) * 100
        + u64::from(hits) * 3
        + u64::from(ships_sunk) * 15;
    let total_xp = result_xp
        .saturating_add(
            rewards
                .iter()
                .map(|reward| u64::from(reward.xp))
                .sum::<u64>(),
        )
        .saturating_add(
            ranked
                .as_ref()
                .map_or(0, |profile| profile.reward_xp_earned),
        );
    const XP_PER_LEVEL: u64 = 500;
    let level = (total_xp / XP_PER_LEVEL + 1).min(100) as u32;
    let level_xp = if level == 100 {
        XP_PER_LEVEL
    } else {
        total_xp % XP_PER_LEVEL
    };
    let xp_to_next_level = if level == 100 {
        0
    } else {
        XP_PER_LEVEL - level_xp
    };
    let rank_title = match level {
        1..=4 => "CADET",
        5..=14 => "LIEUTENANT",
        15..=29 => "COMMANDER",
        30..=49 => "CAPTAIN",
        50..=74 => "COMMODORE",
        _ => "ADMIRAL",
    }
    .to_string();
    let accuracy_percent = if shots == 0 {
        0
    } else {
        ((u64::from(hits) * 100) / u64::from(shots)) as u32
    };
    let achievement =
        |id, title, description, progress: u32, target: u32, unlocked: bool| AchievementProgress {
            id,
            title,
            description,
            progress,
            target,
            unlocked,
        };
    let mission = |id: &'static str,
                   cadence,
                   title: &'static str,
                   description: &'static str,
                   progress: u32,
                   target: u32,
                   reward_xp: u32| {
        let period_key = mission_period_key(cadence, now);
        let claimed_reward = rewards
            .iter()
            .find(|reward| reward.mission_id == id && reward.period_key == period_key);
        let claimed = claimed_reward.is_some();
        MissionProgress {
            id,
            cadence,
            title,
            description,
            progress,
            target,
            reward_xp: claimed_reward.map_or(reward_xp, |reward| reward.xp),
            completed: progress >= target,
            claimed,
            claimable: session.account_id.is_some() && progress >= target && !claimed,
        }
    };
    PlayerProgression {
        account_id: session.account_id,
        handle: session.nickname.clone(),
        level,
        rank_title,
        total_xp,
        level_xp,
        xp_to_next_level,
        games_played,
        wins,
        losses,
        total_shots: shots,
        total_hits: hits,
        total_ships_sunk: ships_sunk,
        ranked,
        achievements: vec![
            achievement(
                "FIRST_CONTACT",
                "첫 접촉",
                "첫 번째 교전을 완료했습니다.",
                games_played,
                1,
                games_played >= 1,
            ),
            achievement(
                "FIRST_VICTORY",
                "첫 승전보",
                "첫 번째 승리를 기록했습니다.",
                wins,
                1,
                wins >= 1,
            ),
            achievement(
                "FLEET_BREAKER",
                "함대 파쇄자",
                "적 함선 25척을 격침했습니다.",
                ships_sunk,
                25,
                ships_sunk >= 25,
            ),
            achievement(
                "SHARPSHOOTER",
                "명사수",
                "20발 이상 사격하고 누적 명중률 60%를 달성했습니다.",
                accuracy_percent,
                60,
                shots >= 20 && accuracy_percent >= 60,
            ),
            achievement(
                "VETERAN",
                "베테랑 지휘관",
                "교전 25회를 완료했습니다.",
                games_played,
                25,
                games_played >= 25,
            ),
        ],
        missions: live_content
            .feature_flags
            .missions_enabled
            .then(|| {
                vec![
                    mission(
                        "DAILY_DEPLOYMENT",
                        MissionCadence::Daily,
                        "오늘의 출항",
                        "오늘 교전 1회를 완료하십시오.",
                        daily_games,
                        1,
                        live_content.tuning.daily_deployment_reward_xp,
                    ),
                    mission(
                        "DAILY_ACCURACY",
                        MissionCadence::Daily,
                        "정밀 포격",
                        "오늘 적 함선 칸 10개를 명중시키십시오.",
                        daily_hits,
                        10,
                        live_content.tuning.daily_accuracy_reward_xp,
                    ),
                    mission(
                        "WEEKLY_SUPREMACY",
                        MissionCadence::Weekly,
                        "주간 제해권",
                        "이번 주 교전 3회에서 승리하십시오.",
                        weekly_wins,
                        3,
                        live_content.tuning.weekly_supremacy_reward_xp,
                    ),
                ]
            })
            .unwrap_or_default(),
        live_content: LiveContentView::from_revision(live_content, now),
        calculated_at: now,
    }
}

pub(super) fn mission_period_key(cadence: MissionCadence, now: chrono::DateTime<Utc>) -> String {
    match cadence {
        MissionCadence::Daily => now.format("%Y-%m-%d").to_string(),
        MissionCadence::Weekly => {
            let week = now.date_naive().iso_week();
            format!("{}-W{:02}", week.year(), week.week())
        }
    }
}

pub(super) fn validate_nickname(nickname: &str) -> Result<(), GameError> {
    let count = nickname.chars().count();
    let valid = (2..=16).contains(&count)
        && nickname.chars().all(|character| {
            character.is_alphanumeric() || character == ' ' || character == '_' || character == '-'
        });
    if valid {
        Ok(())
    } else {
        Err(GameError::InvalidNickname)
    }
}

pub(super) fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}
