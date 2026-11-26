//! OAuth grants and the access/refresh tokens issued under them.

use sqlx::{PgConnection, PgExecutor, Row};
use starstar_shared::Role;
use starstar_shared::auth::{AppGrant, TokenResponse};

use super::{AuthStore, AuthStoreError, GrantError};
use crate::auth::Caller;
use crate::auth::crypto::{hash_token, random_token};

pub const ACCESS_TTL: i64 = 3600;
pub const REFRESH_TTL: i64 = 30 * 86400;
pub const GRANT_TTL: i64 = 90 * 86400;
pub const LAST_USED_THROTTLE: i64 = 60;
/// A rotated refresh token may mint one more pair within this window.
pub const REFRESH_GRACE_SECS: i64 = 30;

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

pub(crate) async fn create_grant_in<'e, E: PgExecutor<'e>>(
    exec: E,
    now: i64,
    user_id: i64,
    client_id: &str,
    role: Role,
    resource: Option<&str>,
) -> Result<String, AuthStoreError> {
    let id = random_token("ss_gr_");
    sqlx::query(
        "INSERT INTO oauth_grants (id, user_id, client_id, role, resource, created_at, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(client_id)
    .bind(role.as_str())
    .bind(resource)
    .bind(now)
    .bind(now + GRANT_TTL)
    .execute(exec)
    .await?;
    Ok(id)
}

/// Mints an access and a refresh token on `grant_id`.
pub(crate) async fn issue_tokens_in(
    conn: &mut PgConnection,
    now: i64,
    grant_id: &str,
) -> Result<IssuedTokens, AuthStoreError> {
    let row = sqlx::query("SELECT role, expires_at FROM oauth_grants WHERE id = $1")
        .bind(grant_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| AuthStoreError::Corrupt("grant vanished".to_owned()))?;
    let role: String = row.try_get(0)?;
    let grant_expires: i64 = row.try_get(1)?;

    let access = random_token("ss_at_");
    let refresh = random_token("ss_rt_");
    let access_exp = (now + ACCESS_TTL).min(grant_expires);
    let refresh_exp = (now + REFRESH_TTL).min(grant_expires);
    for (token, kind, exp) in [
        (&access, "access", access_exp),
        (&refresh, "refresh", refresh_exp),
    ] {
        sqlx::query(
            "INSERT INTO oauth_tokens (token_hash, grant_id, kind, created_at, expires_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(hash_token(token).as_slice())
        .bind(grant_id)
        .bind(kind)
        .bind(now)
        .bind(exp)
        .execute(&mut *conn)
        .await?;
    }
    Ok(IssuedTokens {
        access_token: access,
        refresh_token: refresh,
        expires_in: access_exp - now,
        role: parse_role(&role)?,
    })
}

impl AuthStore {
    pub async fn create_grant(
        &self,
        user_id: i64,
        client_id: &str,
        role: Role,
        resource: Option<&str>,
    ) -> Result<String, AuthStoreError> {
        create_grant_in(
            self.db.pool(),
            self.now(),
            user_id,
            client_id,
            role,
            resource,
        )
        .await
    }

    pub async fn issue_tokens(&self, grant_id: &str) -> Result<IssuedTokens, AuthStoreError> {
        let mut tx = self.db.pool().begin().await?;
        let tokens = issue_tokens_in(&mut tx, self.now(), grant_id).await?;
        tx.commit().await?;
        Ok(tokens)
    }

    /// Resolves an access token to its caller. The role is the lower of the
    /// grant's role and the user's *current* role; a disabled user, an
    /// expired token or an expired grant resolve to `None`.
    pub async fn access_token_caller(&self, token: &str) -> Result<Option<Caller>, AuthStoreError> {
        let now = self.now();
        let found = sqlx::query(
            "SELECT g.id, g.role, g.last_used_at, u.username, u.role, u.disabled
             FROM oauth_tokens t
             JOIN oauth_grants g ON g.id = t.grant_id
             JOIN users u ON u.id = g.user_id
             WHERE t.token_hash = $1 AND t.kind = 'access' AND t.expires_at > $2 AND g.expires_at > $2",
        )
        .bind(hash_token(token).as_slice())
        .bind(now)
        .fetch_optional(self.db.pool())
        .await?;
        let Some(row) = found else {
            return Ok(None);
        };
        let grant_id: String = row.try_get(0)?;
        let grant_role: String = row.try_get(1)?;
        let last_used: Option<i64> = row.try_get(2)?;
        let username: String = row.try_get(3)?;
        let user_role: String = row.try_get(4)?;
        let disabled: bool = row.try_get(5)?;
        if disabled {
            return Ok(None);
        }
        if last_used.is_none_or(|t| t <= now - LAST_USED_THROTTLE) {
            sqlx::query("UPDATE oauth_grants SET last_used_at = $1 WHERE id = $2")
                .bind(now)
                .bind(&grant_id)
                .execute(self.db.pool())
                .await?;
        }
        let role = parse_role(&grant_role)?.min(parse_role(&user_role)?);
        Ok(Some(Caller { username, role }))
    }

    /// Rotates a refresh token into a new pair.
    ///
    /// Locks the parent `oauth_grants` row before locking the `oauth_tokens`
    /// row so concurrent rotations, grace replays, and cascade revocations
    /// serialize in one lock order without deadlocking.
    ///
    /// The first use rotates the token. A replay within [`REFRESH_GRACE_SECS`]
    /// mints one more pair on the same grant (two processes refreshing at
    /// once both succeed). Any further replay, or one after the window,
    /// revokes the whole grant. The grant's role is not recalculated here;
    /// every request caps it by the user's current role.
    pub async fn refresh(
        &self,
        refresh_token: &str,
        client_id: &str,
    ) -> Result<IssuedTokens, GrantError> {
        let mut tx = self.db.pool().begin().await?;
        let now = self.now();
        let hash = hash_token(refresh_token);
        let invalid = || GrantError::Invalid("invalid refresh token".to_owned());

        let grant_id: Option<String> = sqlx::query_scalar(
            "SELECT grant_id FROM oauth_tokens WHERE token_hash = $1 AND kind = 'refresh'",
        )
        .bind(hash.as_slice())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(grant_id) = grant_id else {
            return Err(invalid());
        };

        // Lock the parent grant before the token row to avoid cascade deadlocks.
        let grant_row =
            sqlx::query("SELECT client_id, expires_at FROM oauth_grants WHERE id = $1 FOR UPDATE")
                .bind(&grant_id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(grant_row) = grant_row else {
            return Err(invalid());
        };
        let g_client: String = grant_row.try_get(0)?;
        let g_expires: i64 = grant_row.try_get(1)?;

        let token_row = sqlx::query(
            "SELECT t.expires_at, t.rotated_at, t.grace_used_at, u.disabled
             FROM oauth_tokens t
             JOIN oauth_grants g ON g.id = t.grant_id
             JOIN users u ON u.id = g.user_id
             WHERE t.token_hash = $1 AND t.kind = 'refresh'
             FOR UPDATE OF t",
        )
        .bind(hash.as_slice())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(token_row) = token_row else {
            return Err(invalid());
        };
        let expires_at: i64 = token_row.try_get(0)?;
        let rotated_at: Option<i64> = token_row.try_get(1)?;
        let grace_used_at: Option<i64> = token_row.try_get(2)?;
        let disabled: bool = token_row.try_get(3)?;

        if expires_at <= now || g_expires <= now || g_client != client_id || disabled {
            return Err(invalid());
        }
        match rotated_at {
            None => {
                sqlx::query("UPDATE oauth_tokens SET rotated_at = $1 WHERE token_hash = $2")
                    .bind(now)
                    .bind(hash.as_slice())
                    .execute(&mut *tx)
                    .await?;
            }
            Some(r) if now - r <= REFRESH_GRACE_SECS && grace_used_at.is_none() => {
                sqlx::query("UPDATE oauth_tokens SET grace_used_at = $1 WHERE token_hash = $2")
                    .bind(now)
                    .bind(hash.as_slice())
                    .execute(&mut *tx)
                    .await?;
            }
            Some(_) => {
                sqlx::query("DELETE FROM oauth_grants WHERE id = $1")
                    .bind(&grant_id)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                return Err(GrantError::Invalid(
                    "refresh token reuse detected; the grant was revoked".to_owned(),
                ));
            }
        }
        let tokens = issue_tokens_in(&mut tx, now, &grant_id).await?;
        tx.commit().await?;
        Ok(tokens)
    }

    /// RFC 7009 revocation. Unknown tokens, and tokens of another client than
    /// `client_id`, are silently ignored. A refresh token deletes its grant;
    /// an access token only itself.
    pub async fn revoke_token(
        &self,
        token: &str,
        client_id: Option<&str>,
    ) -> Result<(), AuthStoreError> {
        let hash = hash_token(token);
        let found = sqlx::query(
            "SELECT t.kind, g.id, g.client_id
             FROM oauth_tokens t
             JOIN oauth_grants g ON g.id = t.grant_id
             WHERE t.token_hash = $1",
        )
        .bind(hash.as_slice())
        .fetch_optional(self.db.pool())
        .await?;
        let Some(row) = found else {
            return Ok(());
        };
        let kind: String = row.try_get(0)?;
        let grant_id: String = row.try_get(1)?;
        let g_client: String = row.try_get(2)?;
        if client_id.is_some_and(|c| c != g_client) {
            return Ok(());
        }
        if kind == "refresh" {
            sqlx::query("DELETE FROM oauth_grants WHERE id = $1")
                .bind(grant_id)
                .execute(self.db.pool())
                .await?;
        } else {
            sqlx::query("DELETE FROM oauth_tokens WHERE token_hash = $1")
                .bind(hash.as_slice())
                .execute(self.db.pool())
                .await?;
        }
        Ok(())
    }

    /// Unexpired grants with client and user names, newest first. `None`
    /// means every user.
    pub async fn list_grants(&self, user_id: Option<i64>) -> Result<Vec<AppGrant>, AuthStoreError> {
        let rows = sqlx::query(
            "SELECT g.id, g.client_id, c.name, u.username, g.role, g.resource, g.created_at, g.last_used_at, g.expires_at
             FROM oauth_grants g
             JOIN oauth_clients c ON c.id = g.client_id
             JOIN users u ON u.id = g.user_id
             WHERE g.expires_at > $1 AND ($2::bigint IS NULL OR g.user_id = $2)
             ORDER BY g.created_at DESC, g.id",
        )
        .bind(self.now())
        .bind(user_id)
        .fetch_all(self.db.pool())
        .await?;

        let mut grants = Vec::with_capacity(rows.len());
        for row in rows {
            let role: String = row.try_get(4)?;
            grants.push(AppGrant {
                id: row.try_get(0)?,
                client_id: row.try_get(1)?,
                client_name: row.try_get(2)?,
                username: row.try_get(3)?,
                role: parse_role(&role)?,
                resource: row.try_get(5)?,
                created_at: row.try_get(6)?,
                last_used_at: row.try_get(7)?,
                expires_at: row.try_get(8)?,
            });
        }
        Ok(grants)
    }

    /// Deletes a grant (tokens cascade). With `owner = Some(id)` only a grant
    /// of that user is deleted. Returns whether a row was deleted.
    pub async fn revoke_grant(
        &self,
        grant_id: &str,
        owner: Option<i64>,
    ) -> Result<bool, AuthStoreError> {
        let res = sqlx::query(
            "DELETE FROM oauth_grants WHERE id = $1 AND ($2::bigint IS NULL OR user_id = $2)",
        )
        .bind(grant_id)
        .bind(owner)
        .execute(self.db.pool())
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Deletes every grant of a user (their tokens cascade).
    pub async fn revoke_user_grants(&self, user_id: i64) -> Result<(), AuthStoreError> {
        sqlx::query("DELETE FROM oauth_grants WHERE user_id = $1")
            .bind(user_id)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Hashes of every stored token. For a test that checks nothing is stored
    /// in plain text.
    #[doc(hidden)]
    pub async fn token_hashes(&self) -> Vec<Vec<u8>> {
        sqlx::query_scalar("SELECT token_hash FROM oauth_tokens")
            .fetch_all(self.db.pool())
            .await
            .expect("query token_hashes")
    }
}
