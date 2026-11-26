//! Users and the first-admin / last-admin invariants.

use rusqlite::{Connection, ErrorCode, OptionalExtension, Row, TransactionBehavior, params};
use starstar_shared::Role;
use starstar_shared::auth::UserSummary;

use super::{AuthStore, AuthStoreError};

/// A user row, including the password hash. Not `Debug`.
#[derive(Clone)]
pub struct UserRecord {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub role: Role,
    pub disabled: bool,
    pub created_at: i64,
}

impl UserRecord {
    pub fn summary(&self) -> UserSummary {
        UserSummary {
            username: self.username.clone(),
            role: self.role,
            disabled: self.disabled,
            created_at: self.created_at,
        }
    }
}

const COLUMNS: &str = "id, username, password_hash, role, disabled, created_at";

fn row_to_user(row: &Row<'_>) -> rusqlite::Result<UserRecord> {
    let role: String = row.get(3)?;
    let role = role.parse::<Role>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(UserRecord {
        id: row.get(0)?,
        username: row.get(1)?,
        password_hash: row.get(2)?,
        role,
        disabled: row.get::<_, i64>(4)? != 0,
        created_at: row.get(5)?,
    })
}

pub(crate) fn user_by_id_in(
    conn: &Connection,
    id: i64,
) -> Result<Option<UserRecord>, AuthStoreError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM users WHERE id = ?1"),
            params![id],
            row_to_user,
        )
        .optional()?)
}

pub(crate) fn user_by_name_in(
    conn: &Connection,
    username: &str,
) -> Result<Option<UserRecord>, AuthStoreError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM users WHERE username = ?1"),
            params![username],
            row_to_user,
        )
        .optional()?)
}

fn insert_user_in(
    conn: &Connection,
    username: &str,
    password_hash: &str,
    role: Role,
    now: i64,
) -> Result<UserRecord, AuthStoreError> {
    let result = conn.execute(
        "INSERT INTO users (username, password_hash, role, disabled, created_at) VALUES (?1, ?2, ?3, 0, ?4)",
        params![username, password_hash, role.as_str(), now],
    );
    match result {
        Ok(_) => Ok(UserRecord {
            id: conn.last_insert_rowid(),
            username: username.to_owned(),
            password_hash: password_hash.to_owned(),
            role,
            disabled: false,
            created_at: now,
        }),
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == ErrorCode::ConstraintViolation => {
            Err(AuthStoreError::UsernameTaken(username.to_owned()))
        }
        Err(e) => Err(e.into()),
    }
}

fn enabled_admins_in(conn: &Connection) -> Result<i64, AuthStoreError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM users WHERE role = 'admin' AND disabled = 0",
        [],
        |r| r.get(0),
    )?)
}

impl AuthStore {
    pub fn user_count(&self) -> Result<u64, AuthStoreError> {
        let n: i64 = self
            .db
            .conn()
            .query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
        Ok(n as u64)
    }

    pub fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        role: Role,
    ) -> Result<UserRecord, AuthStoreError> {
        let conn = self.db.conn();
        insert_user_in(&conn, username, password_hash, role, self.now())
    }

    /// Creates the first admin in one `IMMEDIATE` transaction: two concurrent
    /// calls produce exactly one admin and one `SetupClosed`.
    pub fn create_first_admin(
        &self,
        username: &str,
        password_hash: &str,
    ) -> Result<UserRecord, AuthStoreError> {
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
        if count > 0 {
            return Err(AuthStoreError::SetupClosed);
        }
        let user = insert_user_in(&tx, username, password_hash, Role::Admin, self.now())?;
        tx.commit()?;
        Ok(user)
    }

    pub fn user_by_name(&self, username: &str) -> Result<Option<UserRecord>, AuthStoreError> {
        user_by_name_in(&self.db.conn(), username)
    }

    pub fn user_by_id(&self, id: i64) -> Result<Option<UserRecord>, AuthStoreError> {
        user_by_id_in(&self.db.conn(), id)
    }

    pub fn list_users(&self) -> Result<Vec<UserRecord>, AuthStoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM users ORDER BY username"))?;
        let users = stmt
            .query_map([], row_to_user)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(users)
    }

    /// Changes a user's role and/or disabled flag. Refuses (and rolls back) a
    /// change that would leave no enabled admin.
    pub fn update_user(
        &self,
        username: &str,
        role: Option<Role>,
        disabled: Option<bool>,
    ) -> Result<UserRecord, AuthStoreError> {
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let before = user_by_name_in(&tx, username)?
            .ok_or_else(|| AuthStoreError::UserNotFound(username.to_owned()))?;
        let was_enabled_admin = before.role == Role::Admin && !before.disabled;
        if let Some(role) = role {
            tx.execute(
                "UPDATE users SET role = ?1 WHERE id = ?2",
                params![role.as_str(), before.id],
            )?;
        }
        if let Some(disabled) = disabled {
            tx.execute(
                "UPDATE users SET disabled = ?1 WHERE id = ?2",
                params![i64::from(disabled), before.id],
            )?;
        }
        if was_enabled_admin && enabled_admins_in(&tx)? == 0 {
            return Err(AuthStoreError::LastAdmin); // dropping `tx` rolls back
        }
        let after = user_by_id_in(&tx, before.id)?
            .ok_or_else(|| AuthStoreError::UserNotFound(username.to_owned()))?;
        tx.commit()?;
        Ok(after)
    }

    /// Deletes a user; sessions, grants, tokens and codes cascade.
    pub fn remove_user(&self, username: &str) -> Result<(), AuthStoreError> {
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let user = user_by_name_in(&tx, username)?
            .ok_or_else(|| AuthStoreError::UserNotFound(username.to_owned()))?;
        let was_enabled_admin = user.role == Role::Admin && !user.disabled;
        tx.execute("DELETE FROM users WHERE id = ?1", params![user.id])?;
        if was_enabled_admin && enabled_admins_in(&tx)? == 0 {
            return Err(AuthStoreError::LastAdmin);
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_password(&self, user_id: i64, password_hash: &str) -> Result<(), AuthStoreError> {
        self.db.conn().execute(
            "UPDATE users SET password_hash = ?1 WHERE id = ?2",
            params![password_hash, user_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::clock::Clock;

    fn store() -> AuthStore {
        AuthStore::open_in_memory(Clock::manual(1_000)).unwrap()
    }

    #[test]
    fn create_and_find_user() {
        let s = store();
        let u = s.create_user("kerry", "h", Role::Admin).unwrap();
        assert_eq!(u.created_at, 1_000);
        assert_eq!(s.user_count().unwrap(), 1);
        assert_eq!(s.user_by_name("kerry").unwrap().unwrap().role, Role::Admin);
        assert!(matches!(
            s.create_user("kerry", "h", Role::Viewer),
            Err(AuthStoreError::UsernameTaken(_))
        ));
    }

    #[test]
    fn last_enabled_admin_is_guarded() {
        let s = store();
        s.create_user("a", "h", Role::Admin).unwrap();
        s.create_user("v", "h", Role::Viewer).unwrap();
        assert!(matches!(
            s.update_user("a", Some(Role::Operator), None),
            Err(AuthStoreError::LastAdmin)
        ));
        assert!(matches!(
            s.update_user("a", None, Some(true)),
            Err(AuthStoreError::LastAdmin)
        ));
        assert!(matches!(s.remove_user("a"), Err(AuthStoreError::LastAdmin)));
        let a = s.user_by_name("a").unwrap().unwrap();
        assert_eq!(a.role, Role::Admin);
        assert!(!a.disabled); // rolled back
        s.update_user("v", Some(Role::Admin), None).unwrap();
        s.update_user("a", None, Some(true)).unwrap(); // now allowed
        assert!(matches!(s.remove_user("v"), Err(AuthStoreError::LastAdmin))); // a is disabled
        assert!(matches!(
            s.update_user("ghost", None, Some(true)),
            Err(AuthStoreError::UserNotFound(_))
        ));
    }

    #[test]
    fn first_admin_only_once() {
        let s = store();
        s.create_first_admin("a", "h").unwrap();
        assert!(matches!(
            s.create_first_admin("b", "h"),
            Err(AuthStoreError::SetupClosed)
        ));
    }

    #[test]
    fn setup_code_comparison() {
        let s = store();
        assert!(!s.setup_code_matches("anything"));
        s.set_setup_code(Some("ABCDE-FGHJK".into()));
        assert!(s.setup_code_matches("abcde fghjk"));
        assert!(!s.setup_code_matches("ABCDE-FGHJX"));
        s.set_setup_code(None);
        assert!(!s.setup_code_matches("ABCDE-FGHJK"));
    }

    #[test]
    fn removing_a_user_cascades_on_an_in_memory_db() {
        let s = store();
        s.create_user("a", "h", Role::Admin).unwrap();
        let v = s.create_user("v", "h", Role::Viewer).unwrap();
        s.db.conn()
            .execute(
                "INSERT INTO sessions VALUES (x'01', ?1, 'c', 0, 99)",
                params![v.id],
            )
            .unwrap();
        s.remove_user("v").unwrap();
        let n: i64 =
            s.db.conn()
                .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
                .unwrap();
        assert_eq!(n, 0);
    }
}
