//! Persistent registry of printers known to the API, backed by SQLite.

use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use rusqlite::types::Type;
use rusqlite::{Connection, ErrorCode, Row, TransactionBehavior, params};
use starstar_shared::{PrinterKind, PrinterSummary};

use crate::prusalink::PrusaLinkClient;

/// A registered printer, including the credentials used to reach it.
///
/// Not `Debug` on purpose, so the password cannot end up in logs.
#[derive(Clone)]
pub struct StoredPrinter {
    pub name: String,
    pub kind: PrinterKind,
    pub host: String,
    pub username: String,
    pub password: String,
}

impl StoredPrinter {
    pub fn summary(&self) -> PrinterSummary {
        PrinterSummary {
            name: self.name.clone(),
            kind: self.kind,
            host: self.host.clone(),
            username: self.username.clone(),
        }
    }

    pub fn client(&self, http: reqwest::Client) -> PrusaLinkClient {
        PrusaLinkClient::new(http, &self.host, &self.username, &self.password)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("printer `{0}` already exists")]
    Duplicate(String),
    #[error("could not create the database file: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

/// Schema migrations. `PRAGMA user_version` holds how many have been applied.
/// Every statement is `IF NOT EXISTS`, so a database created before versioning
/// existed (user_version 0, `printers` present) migrates cleanly.
const MIGRATIONS: &[&str] = &[
    // 0: printers
    "CREATE TABLE IF NOT EXISTS printers (
        name     TEXT PRIMARY KEY NOT NULL,
        kind     TEXT NOT NULL,
        host     TEXT NOT NULL,
        username TEXT NOT NULL,
        password TEXT NOT NULL
    );",
    // 1: auth
    "CREATE TABLE IF NOT EXISTS users (
        id INTEGER PRIMARY KEY, username TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL,
        role TEXT NOT NULL, disabled INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS sessions (
        token_hash BLOB PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        csrf_token TEXT NOT NULL, created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS oauth_clients (
        id TEXT PRIMARY KEY, name TEXT NOT NULL, redirect_uris TEXT NOT NULL,
        seeded INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS oauth_grants (
        id TEXT PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        client_id TEXT NOT NULL REFERENCES oauth_clients(id) ON DELETE CASCADE,
        role TEXT NOT NULL, resource TEXT, created_at INTEGER NOT NULL,
        last_used_at INTEGER, expires_at INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS oauth_tokens (
        token_hash BLOB PRIMARY KEY, grant_id TEXT NOT NULL REFERENCES oauth_grants(id) ON DELETE CASCADE,
        kind TEXT NOT NULL CHECK (kind IN ('access','refresh')), created_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL, rotated_at INTEGER, grace_used_at INTEGER);
    CREATE TABLE IF NOT EXISTS oauth_codes (
        code_hash BLOB PRIMARY KEY, client_id TEXT NOT NULL REFERENCES oauth_clients(id) ON DELETE CASCADE,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, role TEXT NOT NULL,
        redirect_uri TEXT NOT NULL, code_challenge TEXT NOT NULL, resource TEXT,
        expires_at INTEGER NOT NULL, used_at INTEGER, grant_id TEXT);",
];

fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for sql in MIGRATIONS.iter().skip(version as usize) {
        tx.execute_batch(sql)?;
    }
    if (version as usize) < MIGRATIONS.len() {
        tx.pragma_update(None, "user_version", MIGRATIONS.len() as i64)?;
    }
    tx.commit()
}

/// One SQLite connection shared by the printer registry and the auth tables.
///
/// Not `Debug`. Lock it once per operation and never across an `.await`.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// Opens (creating if needed) the database at `path` and migrates it.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        create_private_file(path)?;
        Self::with_connection(Connection::open(path)?)
    }

    /// An empty, non-persistent database with the same pragmas and migrations.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::with_connection(Connection::open_in_memory()?)
    }

    fn with_connection(mut conn: Connection) -> Result<Self, StoreError> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// The connection. A poisoned lock is recovered: SQLite state is intact.
    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Printers registered with the API, stored in a SQLite database.
///
/// The database holds passwords, so on Unix it is created with owner-only permissions.
pub struct PrinterStore {
    db: Db,
}

impl PrinterStore {
    /// Opens (creating if needed) the database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        Ok(Self::from_db(Db::open(path)?))
    }

    /// An empty, non-persistent registry. Useful for tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Ok(Self::from_db(Db::open_in_memory()?))
    }

    /// A registry over a connection shared with the auth store.
    pub fn from_db(db: Db) -> Self {
        Self { db }
    }

    pub fn list(&self) -> Result<Vec<StoredPrinter>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT name, kind, host, username, password FROM printers ORDER BY name")?;
        let printers = stmt
            .query_map([], row_to_printer)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(printers)
    }

    pub fn get(&self, name: &str) -> Result<Option<StoredPrinter>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT name, kind, host, username, password FROM printers WHERE name = ?1")?;
        let mut rows = stmt.query_map(params![name], row_to_printer)?;
        match rows.next() {
            Some(printer) => Ok(Some(printer?)),
            None => Ok(None),
        }
    }

    pub fn insert(&self, printer: StoredPrinter) -> Result<(), StoreError> {
        let conn = self.conn();
        let result = conn.execute(
            "INSERT INTO printers (name, kind, host, username, password) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                printer.name,
                printer.kind.as_str(),
                printer.host,
                printer.username,
                printer.password
            ],
        );

        match result {
            Ok(_) => Ok(()),
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == ErrorCode::ConstraintViolation =>
            {
                Err(StoreError::Duplicate(printer.name))
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Returns `Ok(false)` when no printer has that name.
    pub fn remove(&self, name: &str) -> Result<bool, StoreError> {
        let deleted = self
            .conn()
            .execute("DELETE FROM printers WHERE name = ?1", params![name])?;
        Ok(deleted > 0)
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.db.conn()
    }
}

fn row_to_printer(row: &Row<'_>) -> rusqlite::Result<StoredPrinter> {
    let kind: String = row.get(1)?;
    let kind = kind
        .parse::<PrinterKind>()
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(1, Type::Text, Box::new(err)))?;

    Ok(StoredPrinter {
        name: row.get(0)?,
        kind,
        host: row.get(2)?,
        username: row.get(3)?,
        password: row.get(4)?,
    })
}

/// Creates the database file with owner-only permissions if it does not exist yet.
/// SQLite then opens the existing file and keeps its permissions.
fn create_private_file(path: &Path) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    match options.open(path) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printer(name: &str) -> StoredPrinter {
        StoredPrinter {
            name: name.to_owned(),
            kind: PrinterKind::Mk4,
            host: "192.168.1.201".to_owned(),
            username: "maker".to_owned(),
            password: "secret".to_owned(),
        }
    }

    #[test]
    fn insert_rejects_duplicates() {
        let store = PrinterStore::open_in_memory().unwrap();

        store.insert(printer("mk4")).unwrap();
        assert!(matches!(
            store.insert(printer("mk4")),
            Err(StoreError::Duplicate(_))
        ));
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn registry_survives_reopen_and_remove_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("starstar.db");

        let store = PrinterStore::open(&path).unwrap();
        store.insert(printer("mk4")).unwrap();
        store.insert(printer("mini")).unwrap();
        assert!(store.remove("mini").unwrap());
        assert!(!store.remove("mini").unwrap());
        drop(store);

        let reopened = PrinterStore::open(&path).unwrap();
        let names: Vec<String> = reopened
            .list()
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, vec!["mk4".to_owned()]);

        let mk4 = reopened.get("mk4").unwrap().unwrap();
        assert_eq!(mk4.kind, PrinterKind::Mk4);
        assert_eq!(mk4.password, "secret");
    }

    #[test]
    fn migrations_set_user_version_and_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.db");
        let db = Db::open(&p).unwrap();
        let v: i64 = db
            .conn()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, MIGRATIONS.len() as i64);
        drop(db);
        Db::open(&p).unwrap();
    }

    #[test]
    fn migrates_a_version_zero_database_with_printers() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("old.db");
        {
            let conn = Connection::open(&p).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.execute("INSERT INTO printers VALUES ('mk4','mk4','h','u','p')", [])
                .unwrap();
        }
        let db = Db::open(&p).unwrap();
        let printers = PrinterStore::from_db(db.clone());
        assert_eq!(printers.list().unwrap().len(), 1);
        let n: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn in_memory_db_has_foreign_keys_on() {
        let db = Db::open_in_memory().unwrap();
        let on: i64 = db
            .conn()
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(on, 1);
    }

    #[test]
    fn printer_store_and_auth_share_one_db() {
        let db = Db::open_in_memory().unwrap();
        let printers = PrinterStore::from_db(db.clone());
        printers.insert(printer("mk4")).unwrap();
        let n: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM printers", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn get_returns_none_for_unknown_printer() {
        let store = PrinterStore::open_in_memory().unwrap();
        assert!(store.get("nope").unwrap().is_none());
    }

    #[test]
    fn list_returns_printers_sorted_by_name() {
        let store = PrinterStore::open_in_memory().unwrap();
        for name in ["zeta", "alpha", "mk4"] {
            store.insert(printer(name)).unwrap();
        }
        let names: Vec<String> = store.list().unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["alpha", "mk4", "zeta"]);
    }
}
