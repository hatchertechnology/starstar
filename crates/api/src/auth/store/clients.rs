//! OAuth clients: the seeded `starstar-cli` plus dynamically registered ones.

use sqlx::postgres::PgRow;
use sqlx::types::Json;
use sqlx::{PgExecutor, Row};
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

fn row_to_client(row: &PgRow) -> Result<ClientRecord, AuthStoreError> {
    let Json(redirect_uris): Json<Vec<String>> = row.try_get(2)?;
    Ok(ClientRecord {
        id: row.try_get(0)?,
        name: row.try_get(1)?,
        redirect_uris,
        seeded: row.try_get(3)?,
        created_at: row.try_get(4)?,
    })
}

pub(crate) async fn client_in<'e, E: PgExecutor<'e>>(
    exec: E,
    id: &str,
) -> Result<Option<ClientRecord>, AuthStoreError> {
    let row = sqlx::query(
        "SELECT id, name, redirect_uris, seeded, created_at FROM oauth_clients WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(exec)
    .await?;
    row.as_ref().map(row_to_client).transpose()
}

impl AuthStore {
    /// Inserts `starstar-cli` if it is missing.
    pub(crate) async fn seed_clients(&self) -> Result<(), AuthStoreError> {
        let uris = vec![
            "http://127.0.0.1/callback".to_owned(),
            "http://localhost/callback".to_owned(),
            "http://[::1]/callback".to_owned(),
        ];
        sqlx::query(
            "INSERT INTO oauth_clients (id, name, redirect_uris, seeded, created_at)
             VALUES ($1, 'starstar CLI', $2, TRUE, $3)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(CLI_CLIENT_ID)
        .bind(Json(uris))
        .bind(self.now())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    pub async fn register_client(
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
        sqlx::query(
            "INSERT INTO oauth_clients (id, name, redirect_uris, seeded, created_at)
             VALUES ($1, $2, $3, FALSE, $4)",
        )
        .bind(&record.id)
        .bind(&record.name)
        .bind(Json(redirect_uris))
        .bind(record.created_at)
        .execute(self.db.pool())
        .await?;
        Ok(record)
    }

    pub async fn client(&self, id: &str) -> Result<Option<ClientRecord>, AuthStoreError> {
        client_in(self.db.pool(), id).await
    }
}
