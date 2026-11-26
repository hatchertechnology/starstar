//! OAuth clients: the seeded `starstar-cli` plus dynamically registered ones.

use rusqlite::{Connection, OptionalExtension, Row, params};
use starstar_shared::auth::CLI_CLIENT_ID;

use super::{AuthStore, AuthStoreError};
use crate::auth::crypto::random_token;

#[derive(Debug, Clone)]
pub struct ClientRecord {
    pub id: String,
    pub name: String,
    pub redirect_uris: Vec<String>,
    pub seeded: bool,
    pub created_at: i64,
}

fn row_to_client(row: &Row<'_>) -> rusqlite::Result<ClientRecord> {
    let uris: String = row.get(2)?;
    let redirect_uris = serde_json::from_str(&uris).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(ClientRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        redirect_uris,
        seeded: row.get::<_, i64>(3)? != 0,
        created_at: row.get(4)?,
    })
}

pub(crate) fn client_in(
    conn: &Connection,
    id: &str,
) -> Result<Option<ClientRecord>, AuthStoreError> {
    Ok(conn
        .query_row(
            "SELECT id, name, redirect_uris, seeded, created_at FROM oauth_clients WHERE id = ?1",
            params![id],
            row_to_client,
        )
        .optional()?)
}

impl AuthStore {
    /// Inserts `starstar-cli` if it is missing.
    pub(crate) fn seed_clients(&self) -> Result<(), AuthStoreError> {
        let uris = serde_json::to_string(&[
            "http://127.0.0.1/callback",
            "http://localhost/callback",
            "http://[::1]/callback",
        ])
        .expect("a string array serializes");
        self.db.conn().execute(
            "INSERT OR IGNORE INTO oauth_clients (id, name, redirect_uris, seeded, created_at) VALUES (?1, 'starstar CLI', ?2, 1, ?3)",
            params![CLI_CLIENT_ID, uris, self.now()],
        )?;
        Ok(())
    }

    pub fn register_client(
        &self,
        name: &str,
        redirect_uris: &[String],
    ) -> Result<ClientRecord, AuthStoreError> {
        let record = ClientRecord {
            id: random_token("ss_cl_"),
            name: name.to_owned(),
            redirect_uris: redirect_uris.to_vec(),
            seeded: false,
            created_at: self.now(),
        };
        let uris = serde_json::to_string(redirect_uris).expect("a string array serializes");
        self.db.conn().execute(
            "INSERT INTO oauth_clients (id, name, redirect_uris, seeded, created_at) VALUES (?1, ?2, ?3, 0, ?4)",
            params![record.id, record.name, uris, record.created_at],
        )?;
        Ok(record)
    }

    pub fn client(&self, id: &str) -> Result<Option<ClientRecord>, AuthStoreError> {
        client_in(&self.db.conn(), id)
    }
}
