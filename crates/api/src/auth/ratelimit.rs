//! A small in-memory failure counter with a fixed window per key.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

/// Counts failures per key. A key is blocked once it has `max_failures`
/// failures inside one window; success clears it with [`RateLimiter::reset`].
pub struct RateLimiter {
    max_failures: u32,
    window_secs: i64,
    /// key -> (failures, window start)
    entries: Mutex<HashMap<String, (u32, i64)>>,
}

impl RateLimiter {
    pub fn new(max_failures: u32, window_secs: i64) -> Self {
        Self {
            max_failures,
            window_secs,
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<String, (u32, i64)>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn is_blocked(&self, key: &str, now: i64) -> bool {
        self.map()
            .get(key)
            .is_some_and(|&(n, start)| now - start < self.window_secs && n >= self.max_failures)
    }

    pub fn record_failure(&self, key: &str, now: i64) {
        let mut map = self.map();
        let entry = map.entry(key.to_owned()).or_insert((0, now));
        if now - entry.1 >= self.window_secs {
            *entry = (0, now);
        }
        entry.0 += 1;
    }

    /// Atomically reserves one attempt: under a single lock, refuses when the
    /// key is already at its limit, otherwise counts the attempt and allows it.
    /// Call it *before* any `.await`, then [`RateLimiter::reset`] on success, so
    /// parallel requests cannot all pass a check-then-record gap.
    ///
    /// The window is fixed, so up to twice the limit can pass across a window
    /// boundary.
    pub fn try_begin(&self, key: &str, now: i64) -> bool {
        let mut map = self.map();
        let entry = map.entry(key.to_owned()).or_insert((0, now));
        if now - entry.1 >= self.window_secs {
            *entry = (0, now);
        }
        if entry.0 >= self.max_failures {
            return false;
        }
        entry.0 += 1;
        true
    }

    /// Gives back one reserved attempt (the request turned out not to count).
    pub fn refund(&self, key: &str) {
        let mut map = self.map();
        if let Some(entry) = map.get_mut(key) {
            entry.0 = entry.0.saturating_sub(1);
            if entry.0 == 0 {
                map.remove(key);
            }
        }
    }

    /// Counts a hit regardless of outcome and reports whether it is allowed.
    /// Used for limits that count every request (client registration).
    pub fn hit(&self, key: &str, now: i64) -> bool {
        self.try_begin(key, now)
    }

    pub fn reset(&self, key: &str) {
        self.map().remove(key);
    }

    pub fn prune(&self, now: i64) {
        let window = self.window_secs;
        self.map().retain(|_, (_, start)| now - *start < window);
    }

    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.map().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_max_failures_until_window_passes() {
        let l = RateLimiter::new(5, 900);
        for _ in 0..5 {
            assert!(!l.is_blocked("k", 0));
            l.record_failure("k", 0);
        }
        assert!(l.is_blocked("k", 10));
        assert!(!l.is_blocked("other", 10));
        assert!(!l.is_blocked("k", 901));
    }

    #[test]
    fn reset_clears() {
        let l = RateLimiter::new(5, 900);
        for _ in 0..5 {
            l.record_failure("k", 0);
        }
        l.reset("k");
        assert!(!l.is_blocked("k", 1));
    }

    #[test]
    fn prune_drops_old_windows() {
        let l = RateLimiter::new(5, 900);
        l.record_failure("a", 0);
        l.record_failure("b", 800);
        l.prune(901);
        assert_eq!(l.count(), 1);
    }

    #[test]
    fn try_begin_is_atomic_across_threads() {
        let l = std::sync::Arc::new(RateLimiter::new(5, 900));
        let allowed: usize = (0..16)
            .map(|_| {
                let l = l.clone();
                std::thread::spawn(move || (0..10).filter(|_| l.try_begin("k", 0)).count())
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().unwrap())
            .sum();
        assert_eq!(allowed, 5);
        l.reset("k");
        assert!(l.try_begin("k", 1));
    }

    #[test]
    fn refund_gives_back_one_attempt() {
        let l = RateLimiter::new(2, 100);
        assert!(l.try_begin("k", 0));
        assert!(l.try_begin("k", 0));
        l.refund("k");
        assert!(l.try_begin("k", 0));
        assert!(!l.try_begin("k", 0));
        l.refund("absent");
    }

    #[test]
    fn hit_counts_every_call() {
        let l = RateLimiter::new(2, 100);
        assert!(l.hit("k", 0));
        assert!(l.hit("k", 0));
        assert!(!l.hit("k", 0));
    }
}
