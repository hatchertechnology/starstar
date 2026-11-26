//! Injectable time source, so tests can step past every expiry.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Unix seconds, from the system or from a manually advanced counter.
#[derive(Clone)]
pub struct Clock(Source);

#[derive(Clone)]
enum Source {
    System,
    Manual(Arc<AtomicI64>),
}

impl Clock {
    pub fn system() -> Self {
        Self(Source::System)
    }

    pub fn manual(start: i64) -> Self {
        Self(Source::Manual(Arc::new(AtomicI64::new(start))))
    }

    pub fn now(&self) -> i64 {
        match &self.0 {
            Source::System => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            Source::Manual(t) => t.load(Ordering::SeqCst),
        }
    }

    /// Moves a manual clock forward. Panics on the system clock.
    pub fn advance(&self, secs: i64) {
        match &self.0 {
            Source::System => panic!("cannot advance the system clock"),
            Source::Manual(t) => {
                t.fetch_add(secs, Ordering::SeqCst);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_advances_and_clones_share_time() {
        let c = Clock::manual(100);
        let d = c.clone();
        c.advance(5);
        assert_eq!(d.now(), 105);
    }

    #[test]
    fn system_clock_is_recent() {
        assert!(Clock::system().now() > 1_700_000_000);
    }
}
