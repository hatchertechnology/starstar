//! OAuth grants and the access/refresh tokens issued under them.

use rusqlite::{Connection, OptionalExtension, params};
use starstar_shared::Role;
use starstar_shared::auth::{AppGrant, TokenResponse};

use super::{AuthStore, AuthStoreError};
use crate::auth::Caller;
use crate::auth::crypto::{hash_token, random_token};

pub const ACCESS_TTL: i64 = 3600;
pub const REFRESH_TTL: i64 = 30 * 86400;
pub const GRANT_TTL: i64 = 90 * 86400;
pub const LAST_USED_THROTTLE: i64 = 60;

/// A freshly minted token pair. The plaintext tokens exist only here.
/// Not `Debug`.
#[derive(Clone)]
pub struct IssuedTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub role: Role,
}

impl IssuedTokens {
    pub fn into_response_body(self) -> TokenResponse {
        TokenResponse {
            access_token: self.access_token,
            token_type: "Bearer".to_owned(),
            expires_in: self.expires_in.max(0) as u64,
            refresh_token: self.refresh_token,
            scope: self.role.scope().to_owned(),
        }
    }
}

pub(crate) fn parse_role(s: &str) -> Result<Role, AuthStoreError> {
    s.parse::<Role>()
        .map_err(|e| AuthStoreError::Corrupt(e.to_string()))
}

pub(crate) fn create_grant_in(
    conn: &Connection,
    now: i64,
    user_id: i64,
    client_id: &str,
    role: Role,
    resource: Option<&str>,
) -> Result<String, AuthStoreError> {
    let id = random_token("ss_gr_");
    conn.execute(
        "INSERT INTO oauth_grants (id, user_id, client_id, role, resource, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, user_id, client_id, role.as_str(), resource, now, now + GRANT_TTL],
    )?;
    Ok(id)
}

/// Mints an access and a refresh token on `grant_id`.
pub(crate) fn issue_tokens_in(
    conn: &Connection,
    now: i64,
    grant_id: &str,
) -> Result<IssuedTokens, AuthStoreError> {
    let (role, grant_expires): (String, i64) = conn
        .query_row(
            "SELECT role, expires_at FROM oauth_grants WHERE id = ?1",
            params![grant_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AuthStoreError::Corrupt("grant vanished".to_owned())
            }
            other => other.into(),
        })?;
    let access = random_token("ss_at_");
    let refresh = random_token("ss_rt_");
    let access_exp = (now + ACCESS_TTL).min(grant_expires);
    let refresh_exp = (now + REFRESH_TTL).min(grant_expires);
    for (token, kind, exp) in [
        (&access, "access", access_exp),
        (&refresh, "refresh", refresh_exp),
    ] {
        conn.execute(
            "INSERT INTO oauth_tokens (token_hash, grant_id, kind, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![hash_token(token).to_vec(), grant_id, kind, now, exp],
        )?;
    }
    Ok(IssuedTokens {
        access_token: access,
        refresh_token: refresh,
        expires_in: access_exp - now,
        role: parse_role(&role)?,
    })
}

impl AuthStore {
    pub fn create_grant(
        &self,
        user_id: i64,
        client_id: &str,
        role: Role,
        resource: Option<&str>,
    ) -> Result<String, AuthStoreError> {
        create_grant_in(
            &self.db.conn(),
            self.now(),
            user_id,
            client_id,
            role,
            resource,
        )
    }

    pub fn issue_tokens(&self, grant_id: &str) -> Result<IssuedTokens, AuthStoreError> {
        issue_tokens_in(&self.db.conn(), self.now(), grant_id)
    }

    /// Resolves an access token to its caller. The role is the lower of the
    /// grant's role and the user's *current* role; a disabled user, an
    /// expired token or an expired grant resolve to `None`.
    pub fn access_token_caller(&self, token: &str) -> Result<Option<Caller>, AuthStoreError> {
        let conn = self.db.conn();
        let now = self.now();
        let found = conn
            .query_row(
                "SELECT g.id, g.role, g.last_used_at, u.username, u.role, u.disabled
                 FROM oauth_tokens t
                 JOIN oauth_grants g ON g.id = t.grant_id
                 JOIN users u ON u.id = g.user_id
                 WHERE t.token_hash = ?1 AND t.kind = 'access' AND t.expires_at > ?2 AND g.expires_at > ?2",
                params![hash_token(token).to_vec(), now],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((grant_id, grant_role, last_used, username, user_role, disabled)) = found else {
            return Ok(None);
        };
        if disabled != 0 {
            return Ok(None);
        }
        if last_used.is_none_or(|t| t <= now - LAST_USED_THROTTLE) {
            conn.execute(
                "UPDATE oauth_grants SET last_used_at = ?1 WHERE id = ?2",
                params![now, grant_id],
            )?;
        }
        let role = parse_role(&grant_role)?.min(parse_role(&user_role)?);
        Ok(Some(Caller { username, role }))
    }

    /// Unexpired grants with client and user names, newest first. `None`
    /// means every user.
    pub fn list_grants(&self, user_id: Option<i64>) -> Result<Vec<AppGrant>, AuthStoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT g.id, g.client_id, c.name, u.username, g.role, g.resource, g.created_at, g.last_used_at, g.expires_at
             FROM oauth_grants g
             JOIN oauth_clients c ON c.id = g.client_id
             JOIN users u ON u.id = g.user_id
             WHERE g.expires_at > ?1 AND (?2 IS NULL OR g.user_id = ?2)
             ORDER BY g.created_at DESC, g.id",
        )?;
        let rows = stmt.query_map(params![self.now(), user_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, i64>(8)?,
            ))
        })?;
        let mut grants = Vec::new();
        for row in rows {
            let (
                id,
                client_id,
                client_name,
                username,
                role,
                resource,
                created_at,
                last_used_at,
                expires_at,
            ) = row?;
            grants.push(AppGrant {
                id,
                client_id,
                client_name,
                username,
                role: parse_role(&role)?,
                resource,
                created_at,
                last_used_at,
                expires_at,
            });
        }
        Ok(grants)
    }

    /// Deletes a grant (tokens cascade). With `owner = Some(id)` only a grant
    /// of that user is deleted. Returns whether a row was deleted.
    pub fn revoke_grant(&self, grant_id: &str, owner: Option<i64>) -> Result<bool, AuthStoreError> {
        let n = self.db.conn().execute(
            "DELETE FROM oauth_grants WHERE id = ?1 AND (?2 IS NULL OR user_id = ?2)",
            params![grant_id, owner],
        )?;
        Ok(n > 0)
    }

    /// Deletes every grant of a user (their tokens cascade).
    pub fn revoke_user_grants(&self, user_id: i64) -> Result<(), AuthStoreError> {
        self.db.conn().execute(
            "DELETE FROM oauth_grants WHERE user_id = ?1",
            params![user_id],
        )?;
        Ok(())
    }

    /// Hashes of every stored token. For a test that checks nothing is stored
    /// in plain text.
    #[doc(hidden)]
    pub fn token_hashes(&self) -> Vec<Vec<u8>> {
        let conn = self.db.conn();
        let mut stmt = conn
            .prepare("SELECT token_hash FROM oauth_tokens")
            .expect("static SQL");
        stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))
            .expect("query")
            .filter_map(Result::ok)
            .collect()
    }
}
