//! Browser sessions.

use rusqlite::{OptionalExtension, params};

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
    pub fn create_session(&self, user_id: i64) -> Result<NewSession, AuthStoreError> {
        let session = NewSession {
            token: random_token("ss_se_"),
            csrf_token: random_token("ss_cs_"),
        };
        let now = self.now();
        self.db.conn().execute(
            "INSERT INTO sessions (token_hash, user_id, csrf_token, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                hash_token(&session.token).to_vec(),
                user_id,
                session.csrf_token,
                now,
                now + SESSION_TTL
            ],
        )?;
        Ok(session)
    }

    /// The session for `token`, if it has not expired and its user is enabled.
    /// The role is the user's current role.
    pub fn session_caller(&self, token: &str) -> Result<Option<SessionCaller>, AuthStoreError> {
        let found = self
            .db
            .conn()
            .query_row(
                "SELECT u.id, u.username, u.role, s.csrf_token
                 FROM sessions s JOIN users u ON u.id = s.user_id
                 WHERE s.token_hash = ?1 AND s.expires_at > ?2 AND u.disabled = 0",
                params![hash_token(token).to_vec(), self.now()],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((user_id, username, role, csrf_token)) = found else {
            return Ok(None);
        };
        Ok(Some(SessionCaller {
            caller: Caller {
                username,
                role: parse_role(&role)?,
            },
            user_id,
            csrf_token,
        }))
    }

    pub fn delete_session(&self, token: &str) -> Result<(), AuthStoreError> {
        self.db.conn().execute(
            "DELETE FROM sessions WHERE token_hash = ?1",
            params![hash_token(token).to_vec()],
        )?;
        Ok(())
    }

    /// Deletes all of a user's sessions except `keep` (a session token).
    pub fn delete_other_sessions(
        &self,
        user_id: i64,
        keep: Option<&str>,
    ) -> Result<(), AuthStoreError> {
        let keep = keep.map(|t| hash_token(t).to_vec()).unwrap_or_default();
        self.db.conn().execute(
            "DELETE FROM sessions WHERE user_id = ?1 AND token_hash != ?2",
            params![user_id, keep],
        )?;
        Ok(())
    }
}
