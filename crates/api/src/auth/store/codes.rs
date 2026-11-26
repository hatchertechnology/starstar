//! Pending authorization requests (in memory) and authorization codes.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::params;
use starstar_shared::Role;

use super::{AuthStore, AuthStoreError};
use crate::auth::crypto::{hash_token, random_token};

pub const CODE_TTL: i64 = 60;
pub const PENDING_TTL: i64 = 600;
/// At most this many authorizations may wait for consent at once.
pub const PENDING_CAP: usize = 1000;

/// An authorize request that has been validated and awaits the user's
/// decision on the consent screen.
#[derive(Clone)]
pub struct PendingAuthorization {
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub state: Option<String>,
    pub requested_role: Role,
    pub resource: Option<String>,
    pub expires_at: i64,
}

/// In-memory map of pending authorizations.
#[derive(Default)]
pub struct PendingMap(Mutex<HashMap<String, PendingAuthorization>>);

impl PendingMap {
    pub(crate) fn lock(&self) -> MutexGuard<'_, HashMap<String, PendingAuthorization>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What an authorization code is bound to.
pub struct CodeParams<'a> {
    pub client_id: &'a str,
    pub user_id: i64,
    pub role: Role,
    pub redirect_uri: &'a str,
    pub code_challenge: &'a str,
    pub resource: Option<&'a str>,
}

/// The highest recognised `starstar:*` scope in a space-separated list, or
/// operator when none is recognised.
pub fn requested_role(scope: Option<&str>) -> Role {
    scope
        .into_iter()
        .flat_map(str::split_whitespace)
        .filter_map(|s| match s {
            "starstar:viewer" => Some(Role::Viewer),
            "starstar:operator" => Some(Role::Operator),
            "starstar:admin" => Some(Role::Admin),
            _ => None,
        })
        .max()
        .unwrap_or(Role::Operator)
}

impl AuthStore {
    /// Stores a pending request and returns its id, or `None` when
    /// [`PENDING_CAP`] unexpired requests are already waiting.
    pub fn put_pending(&self, mut p: PendingAuthorization) -> Option<String> {
        let now = self.now();
        let mut map = self.pending.lock();
        if map.len() >= PENDING_CAP {
            map.retain(|_, v| v.expires_at > now);
            if map.len() >= PENDING_CAP {
                return None;
            }
        }
        p.expires_at = now + PENDING_TTL;
        let id = random_token("ss_rq_");
        map.insert(id.clone(), p);
        Some(id)
    }

    /// The pending request, unless it has expired.
    pub fn pending(&self, id: &str) -> Option<PendingAuthorization> {
        let now = self.now();
        self.pending
            .lock()
            .get(id)
            .filter(|p| p.expires_at > now)
            .cloned()
    }

    /// Removes and returns the pending request, unless it has expired.
    pub fn take_pending(&self, id: &str) -> Option<PendingAuthorization> {
        let now = self.now();
        self.pending
            .lock()
            .remove(id)
            .filter(|p| p.expires_at > now)
    }

    pub fn prune_pending(&self) -> usize {
        let now = self.now();
        let mut map = self.pending.lock();
        let before = map.len();
        map.retain(|_, v| v.expires_at > now);
        before - map.len()
    }

    #[doc(hidden)]
    pub fn pending_len(&self) -> usize {
        self.pending.lock().len()
    }

    /// Creates a single-use authorization code, valid for [`CODE_TTL`].
    pub fn create_code(&self, p: &CodeParams<'_>) -> Result<String, AuthStoreError> {
        let code = random_token("ss_ac_");
        self.db.conn().execute(
            "INSERT INTO oauth_codes (code_hash, client_id, user_id, role, redirect_uri, code_challenge, resource, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                hash_token(&code).to_vec(),
                p.client_id,
                p.user_id,
                p.role.as_str(),
                p.redirect_uri,
                p.code_challenge,
                p.resource,
                self.now() + CODE_TTL
            ],
        )?;
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::clock::Clock;

    fn pending() -> PendingAuthorization {
        PendingAuthorization {
            client_id: "c".into(),
            redirect_uri: "http://127.0.0.1/cb".into(),
            code_challenge: "x".into(),
            state: None,
            requested_role: Role::Viewer,
            resource: None,
            expires_at: 0,
        }
    }

    #[test]
    fn requested_role_picks_the_highest_known_scope() {
        assert_eq!(requested_role(None), Role::Operator);
        assert_eq!(requested_role(Some("openid starstar:viewer")), Role::Viewer);
        assert_eq!(
            requested_role(Some("starstar:viewer starstar:admin")),
            Role::Admin
        );
        assert_eq!(requested_role(Some("profile")), Role::Operator);
    }

    #[test]
    fn pending_requests_expire_and_are_capped() {
        let s = AuthStore::open_in_memory(Clock::manual(0)).unwrap();
        let id = s.put_pending(pending()).unwrap();
        assert!(s.pending(&id).is_some());
        s.clock().advance(601);
        assert!(s.pending(&id).is_none());
        assert!(s.take_pending(&id).is_none());

        for _ in 0..PENDING_CAP {
            s.put_pending(pending()).unwrap();
        }
        assert!(s.put_pending(pending()).is_none());
        s.clock().advance(601); // all expired: room again
        assert!(s.put_pending(pending()).is_some());
    }
}
