//! SQLite-backed auth state, plus small in-memory structures (pending
//! authorizations, rate limiters).
//!
//! **Locking rule:** every public method locks the shared connection once and
//! calls `*_in(conn: &Connection, ...)` helpers. A helper never locks, so no
//! method can deadlock itself, and nothing is held across an `.await`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::Semaphore;

use rusqlite::{TransactionBehavior, params};

use crate::store::{Db, StoreError};

use super::clock::Clock;
use super::crypto::{ct_eq, normalize_setup_code};
use super::password::Hasher;
use super::ratelimit::RateLimiter;

pub mod clients;
pub mod codes;
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
    /// No argon2 permit came free in time; the caller should retry shortly.
    #[error("too many password checks are running, try again shortly")]
    Busy,
}

/// How long a request waits for an argon2 permit before it gets a 503.
pub const ARGON_WAIT: Duration = Duration::from_secs(5);

/// Why a code exchange or refresh failed.
#[derive(Debug, thiserror::Error)]
pub enum GrantError {
    /// The code or token is not usable; the message is safe to show.
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Store(#[from] AuthStoreError),
}

impl From<rusqlite::Error> for GrantError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Store(e.into())
    }
}

/// 4 permits, or the number of CPUs when that is smaller.
fn argon_permit_count() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(4)
}

/// Users, sessions, OAuth clients, grants and tokens.
pub struct AuthStore {
    pub(crate) db: Db,
    pub(crate) clock: Clock,
    pub(crate) hasher: Hasher,
    setup_code: Mutex<Option<String>>,
    login_limiter: RateLimiter,
    login_ip_limiter: RateLimiter,
    authorize_limiter: RateLimiter,
    argon_permits: Arc<Semaphore>,
    argon_wait_ms: AtomicU64,
    setup_limiter: RateLimiter,
    register_limiter: RateLimiter,
    token_limiter: RateLimiter,
    pub(crate) pending: codes::PendingMap,
}

impl AuthStore {
    pub fn new(db: Db, clock: Clock, hasher: Hasher) -> Result<Self, AuthStoreError> {
        let store = Self {
            db,
            clock,
            hasher,
            setup_code: Mutex::new(None),
            login_limiter: RateLimiter::new(5, 900),
            login_ip_limiter: RateLimiter::new(30, 900),
            authorize_limiter: RateLimiter::new(30, 600),
            argon_permits: Arc::new(Semaphore::new(argon_permit_count())),
            argon_wait_ms: AtomicU64::new(ARGON_WAIT.as_millis() as u64),
            setup_limiter: RateLimiter::new(5, 900),
            register_limiter: RateLimiter::new(20, 3600),
            token_limiter: RateLimiter::new(20, 900),
            pending: codes::PendingMap::default(),
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

    /// A coarse limit of 30 sign-in attempts per client IP per 900 s, so
    /// rotating usernames does not escape the per-user limit. Successful
    /// sign-ins are refunded.
    pub fn login_ip_limiter(&self) -> &RateLimiter {
        &self.login_ip_limiter
    }

    /// 30 authorize requests per client IP per 600 s (every request counts),
    /// so one client cannot fill the pending-authorization table.
    pub fn authorize_limiter(&self) -> &RateLimiter {
        &self.authorize_limiter
    }

    /// The global bound on concurrent argon2 runs. Use [`AuthStore::hash_password`],
    /// [`AuthStore::verify_password`] and [`AuthStore::dummy_verify`] instead of
    /// calling the hasher directly.
    pub fn argon_semaphore(&self) -> Arc<Semaphore> {
        self.argon_permits.clone()
    }

    /// Test hook: how long argon2 work waits for a permit (default
    /// [`ARGON_WAIT`]) before failing with [`AuthStoreError::Busy`].
    #[doc(hidden)]
    pub fn set_argon_wait(&self, wait: Duration) {
        self.argon_wait_ms
            .store(wait.as_millis() as u64, Ordering::Relaxed);
    }

    async fn with_argon_permit<T: Send + 'static>(
        &self,
        work: impl FnOnce(Hasher) -> T + Send + 'static,
    ) -> Result<T, AuthStoreError> {
        // Bounded wait: an unbounded queue would let a flood from many
        // addresses delay every real sign-in without limit.
        let wait = Duration::from_millis(self.argon_wait_ms.load(Ordering::Relaxed));
        let permit = tokio::time::timeout(wait, self.argon_permits.clone().acquire_owned())
            .await
            .map_err(|_| AuthStoreError::Busy)?
            .map_err(|_| AuthStoreError::Corrupt("argon2 limiter closed".to_owned()))?;
        let hasher = self.hasher;
        let result = tokio::task::spawn_blocking(move || {
            let out = work(hasher);
            drop(permit);
            out
        })
        .await
        .map_err(|_| AuthStoreError::Corrupt("argon2 task failed".to_owned()))?;
        Ok(result)
    }

    /// Hashes `password` on the blocking pool, holding a global argon2 permit.
    pub async fn hash_password(&self, password: String) -> Result<String, AuthStoreError> {
        self.with_argon_permit(move |h| h.hash(&password)).await
    }

    /// Verifies `password` against `phc`, holding a global argon2 permit.
    pub async fn verify_password(
        &self,
        password: String,
        phc: String,
    ) -> Result<bool, AuthStoreError> {
        self.with_argon_permit(move |h| h.verify(&password, &phc))
            .await
    }

    /// The timing-equalising verify for unknown users, under the same permits.
    pub async fn dummy_verify(&self, password: String) -> Result<(), AuthStoreError> {
        self.with_argon_permit(move |h| h.dummy_verify(&password))
            .await
    }

    /// 5 failed setup attempts per client IP per 900 s.
    pub fn setup_limiter(&self) -> &RateLimiter {
        &self.setup_limiter
    }

    /// 20 client registrations per client IP per hour.
    pub fn register_limiter(&self) -> &RateLimiter {
        &self.register_limiter
    }

    /// 20 failed token requests per client IP per 900 s.
    pub fn token_limiter(&self) -> &RateLimiter {
        &self.token_limiter
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

    /// Deletes expired sessions, codes, tokens and grants, and clients that
    /// were never used (no grant, no live code) for over 24 hours. Also drops
    /// expired pending requests and stale limiter entries. Returns the number
    /// of database rows deleted.
    pub fn prune(&self) -> Result<usize, AuthStoreError> {
        let now = self.now();
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut deleted = 0;
        for sql in [
            "DELETE FROM sessions WHERE expires_at <= ?1",
            "DELETE FROM oauth_codes WHERE expires_at <= ?1",
            "DELETE FROM oauth_tokens WHERE expires_at <= ?1",
            "DELETE FROM oauth_grants WHERE expires_at <= ?1",
        ] {
            deleted += tx.execute(sql, params![now])?;
        }
        deleted += tx.execute(
            "DELETE FROM oauth_clients
             WHERE seeded = 0 AND created_at <= ?1 - 86400
               AND NOT EXISTS (SELECT 1 FROM oauth_grants g WHERE g.client_id = oauth_clients.id)
               AND NOT EXISTS (SELECT 1 FROM oauth_codes c WHERE c.client_id = oauth_clients.id AND c.expires_at > ?1)",
            params![now],
        )?;
        tx.commit()?;
        drop(conn);
        self.prune_pending();
        for limiter in [
            &self.login_limiter,
            &self.login_ip_limiter,
            &self.authorize_limiter,
            &self.setup_limiter,
            &self.register_limiter,
            &self.token_limiter,
        ] {
            limiter.prune(now);
        }
        Ok(deleted)
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

#[cfg(test)]
mod tests {
    use starstar_shared::Role;

    use super::*;

    #[test]
    fn prune_removes_only_expired_rows() {
        let s = AuthStore::open_in_memory(Clock::manual(1_000)).unwrap();
        let u = s.create_user("a", "h", Role::Admin).unwrap();
        let sess = s.create_session(u.id).unwrap();
        let g = s
            .create_grant(u.id, "starstar-cli", Role::Viewer, None)
            .unwrap();
        let _t = s.issue_tokens(&g).unwrap();
        let stale = s
            .register_client("old", &["http://127.0.0.1/cb".into()])
            .unwrap();
        s.clock().advance(3601); // the access token has expired, the refresh token hasn't
        assert_eq!(s.prune().unwrap(), 1);
        assert!(s.client(&stale.id).unwrap().is_some()); // younger than 24 h
        s.clock().advance(7 * 86400);
        assert_eq!(s.prune().unwrap(), 2); // the expired session and the stale client
        assert!(s.session_caller(&sess.token).unwrap().is_none());
        assert!(s.client(&stale.id).unwrap().is_none());
        assert!(s.client("starstar-cli").unwrap().is_some());
        assert_eq!(s.list_grants(None).unwrap().len(), 1);
    }

    #[test]
    fn prune_keeps_clients_that_have_grants_or_live_codes() {
        let s = AuthStore::open_in_memory(Clock::manual(1_000)).unwrap();
        let u = s.create_user("a", "h", Role::Admin).unwrap();
        let used = s
            .register_client("used", &["http://127.0.0.1/cb".into()])
            .unwrap();
        s.create_grant(u.id, &used.id, Role::Viewer, None).unwrap();
        s.clock().advance(2 * 86400);
        s.prune().unwrap();
        assert!(s.client(&used.id).unwrap().is_some());
    }
}
