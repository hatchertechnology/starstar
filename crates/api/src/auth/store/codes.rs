//! Pending authorization requests (in memory) and authorization codes.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use sqlx::{Postgres, Row, Transaction};
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

async fn burn_and_fail(
    tx: Transaction<'_, Postgres>,
    message: &str,
) -> Result<IssuedTokens, GrantError> {
    tx.commit().await?;
    Err(GrantError::Invalid(message.to_owned()))
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
    pub async fn create_code(&self, p: &CodeParams<'_>) -> Result<String, AuthStoreError> {
        let code = random_token("ss_ac_");
        sqlx::query(
            "INSERT INTO oauth_codes (code_hash, client_id, user_id, role, redirect_uri, code_challenge, resource, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(hash_token(&code).as_slice())
        .bind(p.client_id)
        .bind(p.user_id)
        .bind(p.role.as_str())
        .bind(p.redirect_uri)
        .bind(p.code_challenge)
        .bind(p.resource)
        .bind(self.now() + CODE_TTL)
        .execute(self.db.pool())
        .await?;
        Ok(code)
    }

    /// Whether `code` is a live (unused, unexpired) code issued to the CLI
    /// client for `user_id`. The copy-code page shows nothing else.
    pub async fn code_shown_to(&self, code: &str, user_id: i64) -> Result<bool, AuthStoreError> {
        let found: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM oauth_codes
             WHERE code_hash = $1 AND client_id = $2 AND user_id = $3
               AND used_at IS NULL AND expires_at > $4",
        )
        .bind(hash_token(code).as_slice())
        .bind(starstar_shared::auth::CLI_CLIENT_ID)
        .bind(user_id)
        .bind(self.now())
        .fetch_optional(self.db.pool())
        .await?;
        Ok(found.is_some())
    }

    /// Exchanges an authorization code for tokens, locking the code row with
    /// `SELECT ... FOR UPDATE` so concurrent exchanges of one code serialize and
    /// produce one success (and replay revokes the created grant).
    ///
    /// A code is burned (marked used) the moment its row is found, before the
    /// client, redirect and verifier checks: a wrong guess spends the code.
    /// Presenting a used code revokes the grant it created.
    pub async fn exchange_code(
        &self,
        code: &str,
        client_id: &str,
        redirect_uri: &str,
        verifier: &str,
    ) -> Result<IssuedTokens, GrantError> {
        let mut tx = self.db.pool().begin().await?;
        let hash = hash_token(code);
        let invalid = |m: &str| GrantError::Invalid(m.to_owned());

        let row = sqlx::query(
            "SELECT client_id, user_id, role, redirect_uri, code_challenge, resource, expires_at, used_at, grant_id
             FROM oauth_codes WHERE code_hash = $1 FOR UPDATE",
        )
        .bind(hash.as_slice())
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            return Err(invalid("invalid authorization code"));
        };
        let now = self.now();
        let c_client: String = row.try_get(0)?;
        let user_id: i64 = row.try_get(1)?;
        let role: String = row.try_get(2)?;
        let c_redirect: String = row.try_get(3)?;
        let challenge: String = row.try_get(4)?;
        let resource: Option<String> = row.try_get(5)?;
        let expires_at: i64 = row.try_get(6)?;
        let used_at: Option<i64> = row.try_get(7)?;
        let grant_id: Option<String> = row.try_get(8)?;

        if expires_at <= now {
            return Err(invalid("authorization code expired"));
        }
        if used_at.is_some() {
            if let Some(grant_id) = grant_id {
                sqlx::query("DELETE FROM oauth_grants WHERE id = $1")
                    .bind(grant_id)
                    .execute(&mut *tx)
                    .await?;
            }
            tx.commit().await?;
            return Err(invalid("authorization code already used"));
        }
        // Burn first; every later failure still commits this.
        sqlx::query("UPDATE oauth_codes SET used_at = $1 WHERE code_hash = $2")
            .bind(now)
            .bind(hash.as_slice())
            .execute(&mut *tx)
            .await?;

        if c_client != client_id || c_redirect != redirect_uri {
            return burn_and_fail(
                tx,
                "client_id or redirect_uri does not match the authorization",
            )
            .await;
        }
        if !is_valid_pkce_verifier(verifier)
            || !ct_eq(pkce_s256(verifier).as_bytes(), challenge.as_bytes())
        {
            return burn_and_fail(tx, "code_verifier does not match the code_challenge").await;
        }
        let Some(user) = user_by_id_in(&mut *tx, user_id)
            .await?
            .filter(|u| !u.disabled)
        else {
            return burn_and_fail(tx, "the user is not available").await;
        };
        let role = parse_role(&role)?.min(user.role);
        let new_grant =
            create_grant_in(&mut *tx, now, user_id, client_id, role, resource.as_deref()).await?;
        sqlx::query("UPDATE oauth_codes SET grant_id = $1 WHERE code_hash = $2")
            .bind(&new_grant)
            .bind(hash.as_slice())
            .execute(&mut *tx)
            .await?;
        let tokens = issue_tokens_in(&mut tx, now, &new_grant).await?;
        tx.commit().await?;
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

    #[tokio::test]
    async fn pending_requests_expire_and_are_capped() {
        let s = AuthStore::connect_test(Clock::manual(0)).await.unwrap();
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
