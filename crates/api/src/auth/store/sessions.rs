//! Browser sessions.

use sqlx::Row;

use super::grants::parse_role;
use super::{AuthStore, AuthStoreError};
use crate::auth::Caller;
use crate::auth::crypto::{hash_token, random_token};

pub const SESSION_TTL: i64 = 7 * 86400;

/// The plaintext tokens of a session that was just created. Not `Debug`.
pub struct NewSession {
    pub token: String,
    pub csrf_token: String,
}

/// A resolved browser session. Not `Debug`.
#[derive(Clone)]
pub struct SessionCaller {
    pub caller: Caller,
    pub user_id: i64,
    pub csrf_token: String,
}

impl AuthStore {
    pub async fn create_session(&self, user_id: i64) -> Result<NewSession, AuthStoreError> {
        let session = NewSession {
            token: random_token("ss_se_"),
            csrf_token: random_token("ss_cs_"),
        };
        let now = self.now();
        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, csrf_token, created_at, expires_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(hash_token(&session.token).as_slice())
        .bind(user_id)
        .bind(&session.csrf_token)
        .bind(now)
        .bind(now + SESSION_TTL)
        .execute(self.db.pool())
        .await?;
        Ok(session)
    }

    /// The session for `token`, if it has not expired and its user is enabled.
    /// The role is the user's current role.
    pub async fn session_caller(
        &self,
        token: &str,
    ) -> Result<Option<SessionCaller>, AuthStoreError> {
        let row = sqlx::query(
            "SELECT u.id, u.username, u.role, s.csrf_token
             FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = $1 AND s.expires_at > $2 AND u.disabled = FALSE",
        )
        .bind(hash_token(token).as_slice())
        .bind(self.now())
        .fetch_optional(self.db.pool())
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let user_id: i64 = row.try_get(0)?;
        let username: String = row.try_get(1)?;
        let role: String = row.try_get(2)?;
        let csrf_token: String = row.try_get(3)?;
        Ok(Some(SessionCaller {
            caller: Caller {
                username,
                role: parse_role(&role)?,
            },
            user_id,
            csrf_token,
        }))
    }

    pub async fn delete_session(&self, token: &str) -> Result<(), AuthStoreError> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(hash_token(token).as_slice())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Deletes all of a user's sessions except `keep` (a session token).
    pub async fn delete_other_sessions(
        &self,
        user_id: i64,
        keep: Option<&str>,
    ) -> Result<(), AuthStoreError> {
        match keep {
            Some(token) => {
                sqlx::query("DELETE FROM sessions WHERE user_id = $1 AND token_hash <> $2")
                    .bind(user_id)
                    .bind(hash_token(token).as_slice())
                    .execute(self.db.pool())
                    .await?;
            }
            None => {
                sqlx::query("DELETE FROM sessions WHERE user_id = $1")
                    .bind(user_id)
                    .execute(self.db.pool())
                    .await?;
            }
        }
        Ok(())
    }
}
