//! Pending authorization requests (in memory) and authorization codes.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::{OptionalExtension, TransactionBehavior, params};
use starstar_shared::Role;

use super::grants::{IssuedTokens, create_grant_in, issue_tokens_in, parse_role};
use super::users::user_by_id_in;
use super::{AuthStore, AuthStoreError, GrantError};
use crate::auth::crypto::{ct_eq, hash_token, is_valid_pkce_verifier, pkce_s256, random_token};

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

impl AuthStore {
    /// Exchanges an authorization code for tokens, in one `IMMEDIATE`
    /// transaction so concurrent exchanges of one code produce one success.
    ///
    /// A code is burned (marked used) the moment its row is found, before the
    /// client, redirect and verifier checks: a wrong guess spends the code.
    /// Presenting a used code revokes the grant it created.
    pub fn exchange_code(
        &self,
        code: &str,
        client_id: &str,
        redirect_uri: &str,
        verifier: &str,
    ) -> Result<IssuedTokens, GrantError> {
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = self.now();
        let invalid = |m: &str| GrantError::Invalid(m.to_owned());

        let row = tx
            .query_row(
                "SELECT client_id, user_id, role, redirect_uri, code_challenge, resource, expires_at, used_at, grant_id
                 FROM oauth_codes WHERE code_hash = ?1",
                params![hash_token(code).to_vec()],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, Option<String>>(5)?,
                        r.get::<_, i64>(6)?,
                        r.get::<_, Option<i64>>(7)?,
                        r.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            c_client,
            user_id,
            role,
            c_redirect,
            challenge,
            resource,
            expires_at,
            used_at,
            grant_id,
        )) = row
        else {
            return Err(invalid("invalid authorization code"));
        };
        if expires_at <= now {
            return Err(invalid("authorization code expired"));
        }
        if used_at.is_some() {
            if let Some(grant_id) = grant_id {
                tx.execute("DELETE FROM oauth_grants WHERE id = ?1", params![grant_id])?;
            }
            tx.commit()?;
            return Err(invalid("authorization code already used"));
        }
        // Burn first; every later failure still commits this.
        tx.execute(
            "UPDATE oauth_codes SET used_at = ?1 WHERE code_hash = ?2",
            params![now, hash_token(code).to_vec()],
        )?;
        let failure =
            |tx: rusqlite::Transaction<'_>, m: &str| -> Result<IssuedTokens, GrantError> {
                tx.commit()?;
                Err(GrantError::Invalid(m.to_owned()))
            };

        if c_client != client_id || c_redirect != redirect_uri {
            return failure(
                tx,
                "client_id or redirect_uri does not match the authorization",
            );
        }
        if !is_valid_pkce_verifier(verifier)
            || !ct_eq(pkce_s256(verifier).as_bytes(), challenge.as_bytes())
        {
            return failure(tx, "code_verifier does not match the code_challenge");
        }
        let Some(user) = user_by_id_in(&tx, user_id)?.filter(|u| !u.disabled) else {
            return failure(tx, "the user is not available");
        };
        let role = parse_role(&role)?.min(user.role);
        let new_grant = create_grant_in(&tx, now, user_id, client_id, role, resource.as_deref())?;
        tx.execute(
            "UPDATE oauth_codes SET grant_id = ?1 WHERE code_hash = ?2",
            params![new_grant, hash_token(code).to_vec()],
        )?;
        let tokens = issue_tokens_in(&tx, now, &new_grant)?;
        tx.commit()?;
        Ok(tokens)
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
