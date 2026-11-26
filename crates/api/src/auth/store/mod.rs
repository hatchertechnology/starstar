//! SQLite-backed auth state, plus small in-memory structures (pending
//! authorizations, rate limiters).
//!
//! **Locking rule:** every public method locks the shared connection once and
//! calls `*_in(conn: &Connection, ...)` helpers. A helper never locks, so no
//! method can deadlock itself, and nothing is held across an `.await`.

use std::sync::{Mutex, PoisonError};

use crate::store::{Db, StoreError};

use super::clock::Clock;
use super::crypto::{ct_eq, normalize_setup_code};
use super::password::Hasher;
use super::ratelimit::RateLimiter;

pub mod clients;
pub mod grants;
pub mod sessions;
pub mod users;

pub use clients::ClientRecord;
pub use grants::IssuedTokens;

pub use users::UserRecord;

/// Errors from the auth store. `Display` never contains secrets or SQL text
/// that echoes a secret.
#[derive(Debug, thiserror::Error)]
pub enum AuthStoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("database error: {0}")]
    Store(#[from] StoreError),
    #[error("stored data is invalid: {0}")]
    Corrupt(String),
    #[error("username `{0}` is taken")]
    UsernameTaken(String),
    #[error("no user named `{0}`")]
    UserNotFound(String),
    #[error("the last enabled admin cannot be demoted, disabled or removed")]
    LastAdmin,
    #[error("setup is closed: an admin already exists")]
    SetupClosed,
}

/// Users, sessions, OAuth clients, grants and tokens.
pub struct AuthStore {
    pub(crate) db: Db,
    pub(crate) clock: Clock,
    pub(crate) hasher: Hasher,
    setup_code: Mutex<Option<String>>,
    login_limiter: RateLimiter,
    setup_limiter: RateLimiter,
}

impl AuthStore {
    pub fn new(db: Db, clock: Clock, hasher: Hasher) -> Result<Self, AuthStoreError> {
        let store = Self {
            db,
            clock,
            hasher,
            setup_code: Mutex::new(None),
            login_limiter: RateLimiter::new(5, 900),
            setup_limiter: RateLimiter::new(5, 900),
        };
        store.seed_clients()?;
        Ok(store)
    }

    /// An empty in-memory store with cheap hashing, for unit tests.
    pub fn open_in_memory(clock: Clock) -> Result<Self, AuthStoreError> {
        Self::new(Db::open_in_memory()?, clock, Hasher::fast_for_tests())
    }

    /// 5 failed sign-ins per (username, IP) per 900 s.
    pub fn login_limiter(&self) -> &RateLimiter {
        &self.login_limiter
    }

    /// 5 failed setup attempts per client IP per 900 s.
    pub fn setup_limiter(&self) -> &RateLimiter {
        &self.setup_limiter
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    pub fn hasher(&self) -> Hasher {
        self.hasher
    }

    pub fn now(&self) -> i64 {
        self.clock.now()
    }

    /// Sets (or clears) the one-time first-admin setup code.
    pub fn set_setup_code(&self, code: Option<String>) {
        *self
            .setup_code
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = code;
    }

    /// Whether `candidate` equals the setup code, ignoring case, dashes and
    /// whitespace. False when no code is set.
    pub fn setup_code_matches(&self, candidate: &str) -> bool {
        let guard = self
            .setup_code
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match guard.as_deref() {
            Some(code) => ct_eq(
                normalize_setup_code(code).as_bytes(),
                normalize_setup_code(candidate).as_bytes(),
            ),
            None => false,
        }
    }
}
