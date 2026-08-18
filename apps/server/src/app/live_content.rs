use super::*;

impl AppState {
    pub(super) async fn active_live_content(
        &self,
        now: chrono::DateTime<Utc>,
    ) -> Result<LiveContentRevision, GameError> {
        Ok(self
            .store
            .active_live_content(now)
            .await?
            .unwrap_or_else(baseline_live_content))
    }

    pub async fn live_content_view(&self) -> Result<LiveContentView, GameError> {
        let now = Utc::now();
        Ok(LiveContentView::from_revision(
            &self.active_live_content(now).await?,
            now,
        ))
    }

    async fn live_content_candidate(
        &self,
        expected_revision: u64,
        payload: LiveContentPayload,
        operator_id: String,
        rolled_back_from_revision: Option<u64>,
        now: chrono::DateTime<Utc>,
    ) -> Result<LiveContentRevision, GameError> {
        let latest = self
            .store
            .latest_live_content()
            .await?
            .unwrap_or_else(baseline_live_content);
        if latest.revision != expected_revision {
            return Err(GameError::LiveContentRevisionConflict);
        }
        let revision = expected_revision
            .checked_add(1)
            .ok_or(GameError::InvalidRequest)?;
        Ok(LiveContentRevision::from_payload(
            revision,
            payload,
            operator_id,
            now,
            rolled_back_from_revision,
        ))
    }

    pub async fn validate_live_content(
        &self,
        expected_revision: u64,
        payload: LiveContentPayload,
        operator_id: String,
    ) -> Result<LiveContentValidation, GameError> {
        let now = Utc::now();
        Ok(self
            .live_content_candidate(expected_revision, payload, operator_id, None, now)
            .await?
            .validate(now))
    }

    pub async fn publish_live_content(
        &self,
        expected_revision: u64,
        payload: LiveContentPayload,
        operator_id: String,
    ) -> Result<LiveContentRevision, GameError> {
        let now = Utc::now();
        let candidate = self
            .live_content_candidate(expected_revision, payload, operator_id, None, now)
            .await?;
        if !candidate.validate(now).valid {
            return Err(GameError::InvalidRequest);
        }
        if !self
            .store
            .commit_live_content(expected_revision, &candidate)
            .await?
        {
            return Err(GameError::LiveContentRevisionConflict);
        }
        self.metrics
            .live_content_published
            .fetch_add(1, Ordering::Relaxed);
        Ok(candidate)
    }

    pub async fn rollback_live_content(
        &self,
        expected_revision: u64,
        target_revision: u64,
        change_note: String,
        operator_id: String,
    ) -> Result<LiveContentRevision, GameError> {
        if target_revision >= expected_revision {
            return Err(GameError::InvalidRequest);
        }
        let target = if target_revision == 0 {
            baseline_live_content()
        } else {
            self.store
                .live_content_revision(target_revision)
                .await?
                .ok_or(GameError::LiveContentRevisionNotFound)?
        };
        let now = Utc::now();
        let payload = target.payload_for_rollback(now, change_note);
        let candidate = self
            .live_content_candidate(
                expected_revision,
                payload,
                operator_id,
                Some(target_revision),
                now,
            )
            .await?;
        if !candidate.validate(now).valid {
            return Err(GameError::InvalidRequest);
        }
        if !self
            .store
            .commit_live_content(expected_revision, &candidate)
            .await?
        {
            return Err(GameError::LiveContentRevisionConflict);
        }
        self.metrics
            .live_content_rollbacks
            .fetch_add(1, Ordering::Relaxed);
        Ok(candidate)
    }

    pub async fn live_content_history(
        &self,
        limit: usize,
    ) -> Result<(u64, Vec<LiveContentRevision>), GameError> {
        let mut revisions = self.store.live_content_history(limit).await?;
        let current_revision = revisions.first().map_or(0, |revision| revision.revision);
        if revisions.len() < limit {
            revisions.push(baseline_live_content());
        }
        Ok((current_revision, revisions))
    }
}
