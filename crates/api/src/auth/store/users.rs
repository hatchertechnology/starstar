//! Users and the first-admin / last-admin invariants.

use sqlx::postgres::PgRow;
use sqlx::{PgExecutor, Postgres, Row, Transaction};
use starstar_shared::Role;
use starstar_shared::auth::UserSummary;

use super::{AuthStore, AuthStoreError};
use crate::store::is_unique_violation;

/// Advisory lock namespace (`"STAS"`) used to serialize first-admin creation
/// and last-enabled-admin mutations per `current_schema()`.
const USER_ADMIN_LOCK_NS: i32 = 0x5354_4153;

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

fn row_to_user(row: &PgRow) -> Result<UserRecord, AuthStoreError> {
    let role: String = row.try_get(3)?;
    let role = role
        .parse::<Role>()
        .map_err(|e| AuthStoreError::Corrupt(e.to_string()))?;
    Ok(UserRecord {
        id: row.try_get(0)?,
        username: row.try_get(1)?,
        password_hash: row.try_get(2)?,
        role,
        disabled: row.try_get(4)?,
        created_at: row.try_get(5)?,
    })
}

async fn lock_user_admin_tx(tx: &mut Transaction<'_, Postgres>) -> Result<(), AuthStoreError> {
    sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext(current_schema()))")
        .bind(USER_ADMIN_LOCK_NS)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(crate) async fn user_by_id_in<'e, E: PgExecutor<'e>>(
    exec: E,
    id: i64,
) -> Result<Option<UserRecord>, AuthStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM users WHERE id = $1");
    let row = sqlx::query(&sql).bind(id).fetch_optional(exec).await?;
    row.as_ref().map(row_to_user).transpose()
}

pub(crate) async fn user_by_name_in<'e, E: PgExecutor<'e>>(
    exec: E,
    username: &str,
) -> Result<Option<UserRecord>, AuthStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM users WHERE username = $1");
    let row = sqlx::query(&sql)
        .bind(username)
        .fetch_optional(exec)
        .await?;
    row.as_ref().map(row_to_user).transpose()
}

async fn insert_user_in<'e, E: PgExecutor<'e>>(
    exec: E,
    username: &str,
    password_hash: &str,
    role: Role,
    now: i64,
) -> Result<UserRecord, AuthStoreError> {
    let result: Result<i64, sqlx::Error> = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, role, disabled, created_at)
         VALUES ($1, $2, $3, FALSE, $4)
         RETURNING id",
    )
    .bind(username)
    .bind(password_hash)
    .bind(role.as_str())
    .bind(now)
    .fetch_one(exec)
    .await;

    match result {
        Ok(id) => Ok(UserRecord {
            id,
            username: username.to_owned(),
            password_hash: password_hash.to_owned(),
            role,
            disabled: false,
            created_at: now,
        }),
        Err(err) if is_unique_violation(&err) => {
            Err(AuthStoreError::UsernameTaken(username.to_owned()))
        }
        Err(err) => Err(err.into()),
    }
}

async fn enabled_admins_in<'e, E: PgExecutor<'e>>(exec: E) -> Result<i64, AuthStoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin' AND disabled = FALSE")
            .fetch_one(exec)
            .await?,
    )
}

impl AuthStore {
    pub async fn user_count(&self) -> Result<u64, AuthStoreError> {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(self.db.pool())
            .await?;
        Ok(n.max(0) as u64)
    }

    pub async fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        role: Role,
    ) -> Result<UserRecord, AuthStoreError> {
        insert_user_in(self.db.pool(), username, password_hash, role, self.now()).await
    }

    /// Creates the first admin under a schema-scoped advisory transaction lock:
    /// two concurrent calls produce exactly one admin and one `SetupClosed`.
    pub async fn create_first_admin(
        &self,
        username: &str,
        password_hash: &str,
    ) -> Result<UserRecord, AuthStoreError> {
        let mut tx = self.db.pool().begin().await?;
        lock_user_admin_tx(&mut tx).await?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&mut *tx)
            .await?;
        if count > 0 {
            return Err(AuthStoreError::SetupClosed);
        }
        let user =
            insert_user_in(&mut *tx, username, password_hash, Role::Admin, self.now()).await?;
        tx.commit().await?;
        Ok(user)
    }

    pub async fn user_by_name(&self, username: &str) -> Result<Option<UserRecord>, AuthStoreError> {
        user_by_name_in(self.db.pool(), username).await
    }

    pub async fn user_by_id(&self, id: i64) -> Result<Option<UserRecord>, AuthStoreError> {
        user_by_id_in(self.db.pool(), id).await
    }

    pub async fn list_users(&self) -> Result<Vec<UserRecord>, AuthStoreError> {
        let sql = format!("SELECT {COLUMNS} FROM users ORDER BY username");
        let rows = sqlx::query(&sql).fetch_all(self.db.pool()).await?;
        rows.iter().map(row_to_user).collect()
    }

    /// Changes a user's role and/or disabled flag under a schema-scoped
    /// advisory transaction lock. Refuses (and rolls back) a change that would
    /// leave no enabled admin.
    pub async fn update_user(
        &self,
        username: &str,
        role: Option<Role>,
        disabled: Option<bool>,
    ) -> Result<UserRecord, AuthStoreError> {
        let mut tx = self.db.pool().begin().await?;
        lock_user_admin_tx(&mut tx).await?;
        let before = user_by_name_in(&mut *tx, username)
            .await?
            .ok_or_else(|| AuthStoreError::UserNotFound(username.to_owned()))?;
        let was_enabled_admin = before.role == Role::Admin && !before.disabled;
        if let Some(role) = role {
            sqlx::query("UPDATE users SET role = $1 WHERE id = $2")
                .bind(role.as_str())
                .bind(before.id)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(disabled) = disabled {
            sqlx::query("UPDATE users SET disabled = $1 WHERE id = $2")
                .bind(disabled)
                .bind(before.id)
                .execute(&mut *tx)
                .await?;
        }
        if was_enabled_admin && enabled_admins_in(&mut *tx).await? == 0 {
            return Err(AuthStoreError::LastAdmin);
        }
        let after = user_by_id_in(&mut *tx, before.id)
            .await?
            .ok_or_else(|| AuthStoreError::UserNotFound(username.to_owned()))?;
        tx.commit().await?;
        Ok(after)
    }

    /// Deletes a user under a schema-scoped advisory transaction lock;
    /// sessions, grants, tokens and codes cascade.
    pub async fn remove_user(&self, username: &str) -> Result<(), AuthStoreError> {
        let mut tx = self.db.pool().begin().await?;
        lock_user_admin_tx(&mut tx).await?;
        let user = user_by_name_in(&mut *tx, username)
            .await?
            .ok_or_else(|| AuthStoreError::UserNotFound(username.to_owned()))?;
        let was_enabled_admin = user.role == Role::Admin && !user.disabled;
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
        if was_enabled_admin && enabled_admins_in(&mut *tx).await? == 0 {
            return Err(AuthStoreError::LastAdmin);
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn set_password(
        &self,
        user_id: i64,
        password_hash: &str,
    ) -> Result<(), AuthStoreError> {
        sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
            .bind(password_hash)
            .bind(user_id)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::clock::Clock;

    async fn store() -> AuthStore {
        AuthStore::connect_test(Clock::manual(1_000)).await.unwrap()
    }

    #[tokio::test]
    async fn create_and_find_user() {
        let s = store().await;
        let u = s.create_user("kerry", "h", Role::Admin).await.unwrap();
        assert_eq!(u.created_at, 1_000);
        assert_eq!(s.user_count().await.unwrap(), 1);
        assert_eq!(
            s.user_by_name("kerry").await.unwrap().unwrap().role,
            Role::Admin
        );
        assert!(matches!(
            s.create_user("kerry", "h", Role::Viewer).await,
            Err(AuthStoreError::UsernameTaken(_))
        ));
    }

    #[tokio::test]
    async fn last_enabled_admin_is_guarded() {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let s = store().await;
            s.create_user("a", "h", Role::Admin).await.unwrap();
            s.create_user("v", "h", Role::Viewer).await.unwrap();
            assert!(matches!(
                s.update_user("a", Some(Role::Operator), None).await,
                Err(AuthStoreError::LastAdmin)
            ));
            assert!(matches!(
                s.update_user("a", None, Some(true)).await,
                Err(AuthStoreError::LastAdmin)
            ));
            assert!(matches!(
                s.remove_user("a").await,
                Err(AuthStoreError::LastAdmin)
            ));
            let a = s.user_by_name("a").await.unwrap().unwrap();
            assert_eq!(a.role, Role::Admin);
            assert!(!a.disabled); // rolled back
            s.update_user("v", Some(Role::Admin), None).await.unwrap();
            s.update_user("a", None, Some(true)).await.unwrap(); // now allowed
            assert!(matches!(
                s.remove_user("v").await,
                Err(AuthStoreError::LastAdmin)
            )); // a is disabled
            assert!(matches!(
                s.update_user("ghost", None, Some(true)).await,
                Err(AuthStoreError::UserNotFound(_))
            ));
        })
        .await
        .expect("last-admin guard hung");
    }

    #[tokio::test]
    async fn first_admin_only_once() {
        let s = store().await;
        s.create_first_admin("a", "h").await.unwrap();
        assert!(matches!(
            s.create_first_admin("b", "h").await,
            Err(AuthStoreError::SetupClosed)
        ));
    }

    #[tokio::test]
    async fn setup_code_comparison() {
        let s = store().await;
        assert!(!s.setup_code_matches("anything"));
        s.set_setup_code(Some("ABCDE-FGHJK".into()));
        assert!(s.setup_code_matches("abcde fghjk"));
        assert!(!s.setup_code_matches("ABCDE-FGHJX"));
        s.set_setup_code(None);
        assert!(!s.setup_code_matches("ABCDE-FGHJK"));
    }

    #[tokio::test]
    async fn removing_a_user_cascades_sessions() {
        let s = store().await;
        s.create_user("a", "h", Role::Admin).await.unwrap();
        let v = s.create_user("v", "h", Role::Viewer).await.unwrap();
        sqlx::query("INSERT INTO sessions VALUES ($1, $2, 'c', 0, 99)")
            .bind(vec![1u8])
            .bind(v.id)
            .execute(s.db.pool())
            .await
            .unwrap();
        s.remove_user("v").await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(s.db.pool())
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn foreign_keys_reject_missing_user() {
        let s = store().await;
        let res = sqlx::query("INSERT INTO sessions VALUES ($1, $2, 'c', 0, 99)")
            .bind(vec![1u8])
            .bind(999_999i64)
            .execute(s.db.pool())
            .await;
        assert!(res.is_err());
    }
}
