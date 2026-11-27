use crate::{
    error::ApiError,
    library::{blobs::Blobs, store::LibraryError},
    store::{StoreError, is_unique_violation},
};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use starstar_shared::{
    FileRole, ModelRevision, Part, PartFile, PrinterFamily, Setup, SetupRevision, SliceTarget,
};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, thiserror::Error)]
pub enum PartsError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    InUse(String),
    #[error("{0}")]
    BadRequest(String),
    #[error(transparent)]
    Library(#[from] LibraryError),
    #[error(transparent)]
    Store(#[from] StoreError),
}
impl From<sqlx::Error> for PartsError {
    fn from(e: sqlx::Error) -> Self {
        if is_unique_violation(&e) {
            Self::Conflict("name already exists".into())
        } else {
            Self::Store(e.into())
        }
    }
}
impl From<PartsError> for ApiError {
    fn from(e: PartsError) -> Self {
        match e {
            PartsError::NotFound(m) => Self::NotFound(m),
            PartsError::Conflict(m) | PartsError::InUse(m) => Self::Conflict(m),
            PartsError::BadRequest(m) => Self::BadRequest(m),
            PartsError::Library(e) => e.into(),
            PartsError::Store(_) => Self::Internal("internal error".into()),
        }
    }
}
#[derive(Default)]
pub struct PartPatch {
    pub name: Option<String>,
    pub notes: Option<Option<String>>,
    pub default_setup_id: Option<Option<i64>>,
}
pub struct PartsStore {
    pub pool: PgPool,
    pub blobs: Arc<Blobs>,
}
pub(super) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
pub(super) fn missing(what: &str) -> PartsError {
    PartsError::NotFound(format!("{what} not found"))
}
pub(super) async fn lock(tx: &mut Transaction<'_, Postgres>, id: i64) -> Result<(), PartsError> {
    let id =
        i32::try_from(id).map_err(|_| PartsError::BadRequest("part id out of range".into()))?;
    sqlx::query("SELECT pg_advisory_xact_lock(3, $1)")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
impl PartsStore {
    pub fn new(pool: PgPool, blobs: Arc<Blobs>) -> Self {
        Self { pool, blobs }
    }
    pub async fn create_part(
        &self,
        name: &str,
        notes: Option<&str>,
        by: &str,
    ) -> Result<Part, PartsError> {
        let mut tx = self.pool.begin().await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO parts(name,notes,created_by,created_at) VALUES($1,$2,$3,$4) RETURNING id",
        )
        .bind(name)
        .bind(notes)
        .bind(by)
        .bind(now())
        .fetch_one(&mut *tx)
        .await?;
        lock(&mut tx, id).await?;
        sqlx::query("UPDATE parts SET next_model_revision=2 WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO model_revisions(part_id,number,created_by,created_at) VALUES($1,1,$2,$3)",
        )
        .bind(id)
        .bind(by)
        .bind(now())
        .execute(&mut *tx)
        .await?;
        let part = self.read_part(&mut tx, id).await?;
        tx.commit().await?;
        Ok(part)
    }
    pub async fn update_part(&self, id: i64, patch: PartPatch) -> Result<Part, PartsError> {
        let mut tx = self.pool.begin().await?;
        lock(&mut tx, id).await?;
        if let Some(Some(setup)) = patch.default_setup_id {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM setups WHERE id=$1 AND part_id=$2)",
            )
            .bind(setup)
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            if !exists {
                return Err(PartsError::BadRequest(
                    "default setup must belong to part".into(),
                ));
            }
        }
        let n=sqlx::query("UPDATE parts SET name=COALESCE($2,name), notes=CASE WHEN $3 THEN $4 ELSE notes END, default_setup_id=CASE WHEN $5 THEN $6 ELSE default_setup_id END WHERE id=$1").bind(id).bind(patch.name).bind(patch.notes.is_some()).bind(patch.notes.flatten()).bind(patch.default_setup_id.is_some()).bind(patch.default_setup_id.flatten()).execute(&mut *tx).await?.rows_affected();
        if n == 0 {
            return Err(missing("part"));
        }
        let part = self.read_part(&mut tx, id).await?;
        tx.commit().await?;
        Ok(part)
    }
    pub async fn new_model_revision(
        &self,
        part_id: i64,
        note: Option<&str>,
        carry: &[i64],
        by: &str,
    ) -> Result<ModelRevision, PartsError> {
        let mut tx = self.pool.begin().await?;
        lock(&mut tx, part_id).await?;
        let latest: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM model_revisions WHERE part_id=$1 ORDER BY number DESC LIMIT 1",
        )
        .bind(part_id)
        .fetch_optional(&mut *tx)
        .await?;
        let number:i32=sqlx::query_scalar("UPDATE parts SET next_model_revision=next_model_revision+1 WHERE id=$1 RETURNING next_model_revision-1").bind(part_id).fetch_optional(&mut *tx).await?.ok_or_else(||missing("part"))?;
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM part_files WHERE id=ANY($1) AND part_id=$2 AND model_revision_id=$3").bind(carry).bind(part_id).bind(latest).fetch_one(&mut *tx).await?;
        if count != carry.len() as i64 {
            return Err(PartsError::BadRequest(
                "carried files must belong to the latest model revision".into(),
            ));
        }
        let id:i64=sqlx::query_scalar("INSERT INTO model_revisions(part_id,number,note,created_by,created_at) VALUES($1,$2,$3,$4,$5) RETURNING id").bind(part_id).bind(number).bind(note).bind(by).bind(now()).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO part_files(part_id,model_revision_id,setup_revision_id,role,name,sha256,size,kind,meta,parsed,has_preview,uploaded_by,uploaded_at,target_family,target_nozzle,profile,note,superseded_by) SELECT part_id,$1,NULL,role,name,sha256,size,kind,meta,parsed,has_preview,uploaded_by,uploaded_at,target_family,target_nozzle,profile,note,NULL FROM part_files WHERE id=ANY($2)").bind(id).bind(carry).execute(&mut *tx).await?;
        let model = self.model(&mut tx, id).await?;
        tx.commit().await?;
        Ok(model)
    }
    pub async fn create_setup(
        &self,
        part_id: i64,
        name: &str,
        by: &str,
    ) -> Result<Setup, PartsError> {
        let mut tx = self.pool.begin().await?;
        lock(&mut tx, part_id).await?;
        let model: i64 = sqlx::query_scalar(
            "SELECT id FROM model_revisions WHERE part_id=$1 ORDER BY number DESC LIMIT 1",
        )
        .bind(part_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| missing("part or model revision"))?;
        let id:i64=sqlx::query_scalar("INSERT INTO setups(part_id,name,created_by,created_at,next_revision) VALUES($1,$2,$3,$4,2) RETURNING id").bind(part_id).bind(name).bind(by).bind(now()).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO setup_revisions(part_id,setup_id,number,model_revision_id,created_by,created_at) VALUES($1,$2,1,$3,$4,$5)").bind(part_id).bind(id).bind(model).bind(by).bind(now()).execute(&mut *tx).await?;
        let setup = self.setup(&mut tx, id).await?;
        tx.commit().await?;
        Ok(setup)
    }
    pub async fn update_setup(
        &self,
        id: i64,
        name: Option<String>,
        default_profile: Option<Option<String>>,
    ) -> Result<Setup, PartsError> {
        let mut tx = self.pool.begin().await?;
        let part: i64 = sqlx::query_scalar("SELECT part_id FROM setups WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| missing("setup"))?;
        lock(&mut tx, part).await?;
        let n=sqlx::query("UPDATE setups SET name=COALESCE($2,name),default_profile=CASE WHEN $3 THEN $4 ELSE default_profile END WHERE id=$1").bind(id).bind(name).bind(default_profile.is_some()).bind(default_profile.flatten()).execute(&mut *tx).await?.rows_affected();
        if n == 0 {
            return Err(missing("setup"));
        }
        let setup = self.setup(&mut tx, id).await?;
        tx.commit().await?;
        Ok(setup)
    }
    pub async fn new_setup_revision(
        &self,
        setup_id: i64,
        model_revision_id: Option<i64>,
        note: Option<&str>,
        by: &str,
    ) -> Result<SetupRevision, PartsError> {
        let mut tx = self.pool.begin().await?;
        let part: i64 = sqlx::query_scalar("SELECT part_id FROM setups WHERE id=$1")
            .bind(setup_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| missing("setup"))?;
        lock(&mut tx, part).await?;
        let model: Option<i64> = if let Some(id) = model_revision_id {
            sqlx::query_scalar("SELECT id FROM model_revisions WHERE id=$1 AND part_id=$2")
                .bind(id)
                .bind(part)
                .fetch_optional(&mut *tx)
                .await?
        } else {
            sqlx::query_scalar(
                "SELECT id FROM model_revisions WHERE part_id=$1 ORDER BY number DESC LIMIT 1",
            )
            .bind(part)
            .fetch_optional(&mut *tx)
            .await?
        };
        let model = model
            .ok_or_else(|| PartsError::BadRequest("model revision must belong to part".into()))?;
        let number: i32 = sqlx::query_scalar(
            "UPDATE setups SET next_revision=next_revision+1 WHERE id=$1 RETURNING next_revision-1",
        )
        .bind(setup_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| missing("setup"))?;
        let id:i64=sqlx::query_scalar("INSERT INTO setup_revisions(part_id,setup_id,number,model_revision_id,note,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id").bind(part).bind(setup_id).bind(number).bind(model).bind(note).bind(by).bind(now()).fetch_one(&mut *tx).await?;
        let revision = self.setup_revision(&mut tx, id).await?;
        tx.commit().await?;
        Ok(revision)
    }
    async fn delete(&self, kind: &str, id: i64) -> Result<(), PartsError> {
        let mut tx = self.pool.begin().await?;
        let (find, files, delete) = match kind {
            "part" => (
                "SELECT id FROM parts WHERE id=$1",
                "SELECT DISTINCT sha256 FROM part_files WHERE part_id=$1",
                "DELETE FROM parts WHERE id=$1",
            ),
            "setup" => (
                "SELECT part_id FROM setups WHERE id=$1",
                "SELECT DISTINCT f.sha256 FROM part_files f JOIN setup_revisions r ON r.id=f.setup_revision_id WHERE r.setup_id=$1",
                "DELETE FROM setups WHERE id=$1",
            ),
            "setup revision" => (
                "SELECT part_id FROM setup_revisions WHERE id=$1",
                "SELECT DISTINCT sha256 FROM part_files WHERE setup_revision_id=$1",
                "DELETE FROM setup_revisions WHERE id=$1",
            ),
            _ => (
                "SELECT part_id FROM model_revisions WHERE id=$1",
                "SELECT DISTINCT sha256 FROM part_files WHERE model_revision_id=$1",
                "DELETE FROM model_revisions WHERE id=$1",
            ),
        };
        let part: i64 = sqlx::query_scalar(find)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| missing(kind))?;
        lock(&mut tx, part).await?;
        if kind == "model revision" {
            let names:Vec<String>=sqlx::query_scalar("SELECT DISTINCT s.name FROM setups s JOIN setup_revisions r ON r.setup_id=s.id WHERE r.model_revision_id=$1 ORDER BY s.name").bind(id).fetch_all(&mut *tx).await?;
            if !names.is_empty() {
                return Err(PartsError::InUse(format!(
                    "model revision is used by setups: {}",
                    names.join(", ")
                )));
            }
        }
        let mut shas: Vec<String> = sqlx::query_scalar(files)
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
        sort_digests(&mut shas);
        for sha in &shas {
            sqlx::query("INSERT INTO library_cleanup(sha256) VALUES($1) ON CONFLICT DO NOTHING")
                .bind(sha)
                .execute(&mut *tx)
                .await?;
        }
        if kind == "part" {
            sqlx::query("DELETE FROM setups WHERE part_id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        let affected = sqlx::query(delete)
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if affected == 0 {
            return Err(missing(kind));
        }
        tx.commit().await?;
        for sha in shas {
            self.blobs.cleanup_digest(&self.pool, &sha).await?;
        }
        Ok(())
    }
    pub async fn delete_part(&self, id: i64) -> Result<(), PartsError> {
        self.delete("part", id).await
    }
    pub async fn delete_setup(&self, id: i64) -> Result<(), PartsError> {
        self.delete("setup", id).await
    }
    pub async fn delete_setup_revision(&self, id: i64) -> Result<(), PartsError> {
        self.delete("setup revision", id).await
    }
    pub async fn delete_model_revision(&self, id: i64) -> Result<(), PartsError> {
        self.delete("model revision", id).await
    }
    pub async fn get_part(&self, id: i64) -> Result<Part, PartsError> {
        let mut tx = self.pool.begin().await?;
        lock(&mut tx, id).await?;
        let part = self.read_part(&mut tx, id).await?;
        tx.commit().await?;
        Ok(part)
    }
    async fn read_part(&self, conn: &mut PgConnection, id: i64) -> Result<Part, PartsError> {
        let row = sqlx::query("SELECT * FROM parts WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or_else(|| missing("part"))?;
        let mids: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM model_revisions WHERE part_id=$1 ORDER BY number")
                .bind(id)
                .fetch_all(&mut *conn)
                .await?;
        let sids: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM setups WHERE part_id=$1 ORDER BY id")
                .bind(id)
                .fetch_all(&mut *conn)
                .await?;
        let mut models = Vec::new();
        for mid in mids {
            models.push(self.model(conn, mid).await?)
        }
        let mut setups = Vec::new();
        for sid in sids {
            setups.push(self.setup(conn, sid).await?)
        }
        let other_files=self.files(conn, "SELECT * FROM part_files WHERE part_id=$1 AND model_revision_id IS NULL AND setup_revision_id IS NULL ORDER BY id",id).await?;
        Ok(Part {
            id,
            name: row.try_get("name")?,
            notes: row.try_get("notes")?,
            default_setup_id: row.try_get("default_setup_id")?,
            created_by: row.try_get("created_by")?,
            created_at: row.try_get("created_at")?,
            preview_file_id: None,
            model_revisions: models,
            setups,
            other_files,
        })
    }
    async fn model(&self, conn: &mut PgConnection, id: i64) -> Result<ModelRevision, PartsError> {
        let row = sqlx::query("SELECT * FROM model_revisions WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or_else(|| missing("model revision"))?;
        Ok(ModelRevision {
            id,
            number: row.try_get("number")?,
            note: row.try_get("note")?,
            created_by: row.try_get("created_by")?,
            created_at: row.try_get("created_at")?,
            files: self
                .files(
                    conn,
                    "SELECT * FROM part_files WHERE model_revision_id=$1 ORDER BY id",
                    id,
                )
                .await?,
        })
    }
    async fn setup(&self, conn: &mut PgConnection, id: i64) -> Result<Setup, PartsError> {
        let row = sqlx::query("SELECT * FROM setups WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or_else(|| missing("setup"))?;
        let ids: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM setup_revisions WHERE setup_id=$1 ORDER BY number")
                .bind(id)
                .fetch_all(&mut *conn)
                .await?;
        let mut revisions = Vec::new();
        for rid in ids {
            revisions.push(self.setup_revision(conn, rid).await?)
        }
        let latest: Option<i32> =
            sqlx::query_scalar("SELECT MAX(number) FROM model_revisions WHERE part_id=$1")
                .bind(row.try_get::<i64, _>("part_id")?)
                .fetch_one(&mut *conn)
                .await?;
        let behind_model = revisions.last().and_then(|revision| {
            latest
                .filter(|latest| *latest > revision.model_revision_number)
                .map(|latest| {
                    format!(
                        "on model rev {}; latest is rev {latest}",
                        revision.model_revision_number
                    )
                })
        });
        Ok(Setup {
            id,
            name: row.try_get("name")?,
            default_profile: row.try_get("default_profile")?,
            revisions,
            behind_model,
        })
    }
    async fn setup_revision(
        &self,
        conn: &mut PgConnection,
        id: i64,
    ) -> Result<SetupRevision, PartsError> {
        let row=sqlx::query("SELECT r.*, m.number AS model_number FROM setup_revisions r JOIN model_revisions m ON m.id=r.model_revision_id WHERE r.id=$1").bind(id).fetch_optional(&mut *conn).await?.ok_or_else(||missing("setup revision"))?;
        let files = self
            .files(
                conn,
                "SELECT * FROM part_files WHERE setup_revision_id=$1 ORDER BY id",
                id,
            )
            .await?;
        let project = files
            .iter()
            .find(|f| f.role == FileRole::Project3mf)
            .cloned();
        let slices = files
            .into_iter()
            .filter(|f| f.role == FileRole::Slice)
            .collect();
        Ok(SetupRevision {
            id,
            number: row.try_get("number")?,
            model_revision_id: row.try_get("model_revision_id")?,
            model_revision_number: row.try_get("model_number")?,
            note: row.try_get("note")?,
            created_by: row.try_get("created_by")?,
            created_at: row.try_get("created_at")?,
            project,
            slices,
        })
    }
    pub(super) async fn files(
        &self,
        conn: &mut PgConnection,
        query: &str,
        id: i64,
    ) -> Result<Vec<PartFile>, PartsError> {
        let rows = sqlx::query(query).bind(id).fetch_all(&mut *conn).await?;
        rows.into_iter()
            .map(|r| {
                let role: String = r.try_get("role")?;
                let role = match role.as_str() {
                    "source" => FileRole::Source,
                    "render" => FileRole::Render,
                    "project_3mf" => FileRole::Project3mf,
                    "slice" => FileRole::Slice,
                    _ => FileRole::Other,
                };
                let family: Option<String> = r.try_get("target_family")?;
                let nozzle: Option<f64> = r.try_get("target_nozzle")?;
                let profile: Option<String> = r.try_get("profile")?;
                let target = if role == FileRole::Slice {
                    Some(SliceTarget {
                        family: family.and_then(|f| match f.as_str() {
                            "mk4" => Some(PrinterFamily::Mk4),
                            "mini" => Some(PrinterFamily::Mini),
                            _ => None,
                        }),
                        nozzle,
                        profile,
                    })
                } else {
                    None
                };
                let meta: Option<sqlx::types::Json<starstar_shared::PrintMeta>> =
                    r.try_get("meta")?;
                Ok(PartFile {
                    id: r.try_get("id")?,
                    part_id: r.try_get("part_id")?,
                    model_revision_id: r.try_get("model_revision_id")?,
                    setup_revision_id: r.try_get("setup_revision_id")?,
                    role,
                    name: r.try_get("name")?,
                    sha256: r.try_get("sha256")?,
                    size: r.try_get::<i64, _>("size")? as u64,
                    meta: meta.map(|v| v.0),
                    parsed: r.try_get("parsed")?,
                    has_preview: r.try_get("has_preview")?,
                    target,
                    superseded_by: r.try_get("superseded_by")?,
                    note: r.try_get("note")?,
                    uploaded_by: r.try_get("uploaded_by")?,
                    uploaded_at: r.try_get("uploaded_at")?,
                })
            })
            .collect()
    }
}

fn sort_digests(digests: &mut [String]) {
    digests.sort_unstable();
}

#[cfg(test)]
mod tests {
    use super::sort_digests;

    #[test]
    fn cleanup_digests_have_a_consistent_order() {
        let mut digests = vec!["c".to_owned(), "a".to_owned(), "b".to_owned()];
        sort_digests(&mut digests);
        assert_eq!(digests, ["a", "b", "c"]);
    }
}
