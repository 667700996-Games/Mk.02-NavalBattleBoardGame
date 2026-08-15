use std::{sync::Arc, time::Duration};

use dashmap::DashMap;
use tokio::time::Instant;

#[derive(Debug, Clone)]
struct Window {
    started_at: Instant,
    requests: u32,
}

/// A bounded in-process limiter used as the first line of defence.
///
/// Production deployments must additionally enforce shared limits at the edge so limits cannot be
/// bypassed by switching application instances. Keeping this limiter in the process still protects
/// each instance from a failed or abusive client when the edge is misconfigured.
#[derive(Debug, Clone)]
pub struct FixedWindowRateLimiter {
    entries: Arc<DashMap<String, Window>>,
    window: Duration,
    max_requests: u32,
    max_keys: usize,
}

impl FixedWindowRateLimiter {
    pub fn new(window: Duration, max_requests: u32, max_keys: usize) -> Self {
        Self {
            entries: Arc::new(DashMap::new()),
            window,
            max_requests,
            max_keys: max_keys.max(1),
        }
    }

    pub fn check(&self, key: impl Into<String>) -> bool {
        if self.max_requests == 0 {
            return true;
        }

        let key = key.into();
        let now = Instant::now();
        if self.entries.len() >= self.max_keys && !self.entries.contains_key(&key) {
            self.entries
                .retain(|_, value| now.saturating_duration_since(value.started_at) < self.window);
            if self.entries.len() >= self.max_keys {
                return false;
            }
        }

        let mut entry = self.entries.entry(key).or_insert(Window {
            started_at: now,
            requests: 0,
        });
        if now.saturating_duration_since(entry.started_at) >= self.window {
            entry.started_at = now;
            entry.requests = 0;
        }
        if entry.requests >= self.max_requests {
            return false;
        }
        entry.requests = entry.requests.saturating_add(1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_requests_after_the_configured_budget() {
        let limiter = FixedWindowRateLimiter::new(Duration::from_secs(60), 2, 16);
        assert!(limiter.check("alpha"));
        assert!(limiter.check("alpha"));
        assert!(!limiter.check("alpha"));
        assert!(limiter.check("bravo"));
    }

    #[test]
    fn zero_budget_disables_the_limiter() {
        let limiter = FixedWindowRateLimiter::new(Duration::from_secs(60), 0, 1);
        for _ in 0..100 {
            assert!(limiter.check("alpha"));
        }
    }
}
