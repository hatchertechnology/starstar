use super::blobs::{Blobs, PreviewSource};
use crate::store::StoreError;
use axum::body::Bytes;
use futures_util::Stream;
use sqlx::{PgPool, Row};
use starstar_shared::{LibraryFile, LibraryKind, LibrarySort};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("incomplete upload")]
    Incomplete,
    #[error("upload exceeds {0} bytes")]
    TooLarge(u64),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Store(#[from] StoreError),
}
impl From<sqlx::Error> for LibraryError {
    fn from(e: sqlx::Error) -> Self {
        Self::Store(e.into())
    }
}
impl From<LibraryError> for crate::error::ApiError {
    fn from(e: LibraryError) -> Self {
        match e {
            LibraryError::Incomplete => Self::BadRequest("incomplete upload".into()),
            LibraryError::TooLarge(max) => {
                Self::PayloadTooLarge(crate::printers::files::too_large_message(max))
            }
            _ => Self::Internal("internal error".into()),
        }
    }
}

pub struct LibraryStore {
    blobs: Arc<Blobs>,
    pool: PgPool,
}
impl LibraryStore {
    pub fn open(dir: &Path, pool: PgPool) -> Result<Self, StoreError> {
        Ok(Self {
            blobs: Arc::new(Blobs::open(dir)?),
            pool,
        })
    }
    pub fn temporary(pool: PgPool) -> Result<Self, StoreError> {
        Ok(Self {
            blobs: Arc::new(Blobs::temporary()?),
            pool,
        })
    }
    pub fn blobs(&self) -> Arc<Blobs> {
        self.blobs.clone()
    }
    pub fn blob_path(&self, f: &LibraryFile) -> PathBuf {
        self.blobs.path(&f.sha256)
    }
    pub fn preview_path(&self, f: &LibraryFile) -> Option<PathBuf> {
        f.has_preview.then(|| self.blobs.preview(&f.sha256))
    }
    pub async fn add<E>(
        &self,
        name: &str,
        kind: LibraryKind,
        uploaded_by: &str,
        body: impl Stream<Item = Result<Bytes, E>> + Unpin,
        expected_len: u64,
        max: u64,
    ) -> Result<(LibraryFile, bool), LibraryError> {
        let ingested = self
            .blobs
            .receive(body, expected_len, max, PreviewSource::Slice(kind))
            .await?;
        let sha = &ingested.sha256;
        let size = ingested.size;
        let parsed = ingested.parsed.as_ref().expect("slice is parsed");
        let mut tx = self.pool.begin().await?;
        // Separate namespaces prevent name/digest lock collisions. All adds acquire name first.
        sqlx::query("SELECT pg_advisory_xact_lock(1, hashtext($1))")
            .bind(name)
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT pg_advisory_xact_lock(2, hashtext($1))")
            .bind(sha)
            .execute(&mut *tx)
            .await?;
        self.blobs.validate_assets(sha)?;
        if let Some(row) =
            sqlx::query("SELECT * FROM library_files WHERE name = $1 ORDER BY version DESC LIMIT 1")
                .bind(name)
                .fetch_optional(&mut *tx)
                .await?
        {
            let existing = row_file(row)?;
            if existing.sha256 == *sha {
                tx.commit().await?;
                return Ok((existing, false));
            }
        }
        self.blobs.publish(&ingested)?;
        let version: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version),0)+1 FROM library_files WHERE name=$1",
        )
        .bind(name)
        .fetch_one(&mut *tx)
        .await?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let meta = if parsed.parsed {
            Some(serde_json::to_value(&parsed.meta).map_err(io::Error::other)?)
        } else {
            None
        };
        let row = sqlx::query("INSERT INTO library_files (name,version,sha256,size,kind,meta,parsed,has_preview,uploaded_by,uploaded_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING *")
            .bind(name).bind(version).bind(sha).bind(size as i64).bind(match kind { LibraryKind::Bgcode => "bgcode", LibraryKind::Gcode => "gcode" }).bind(meta).bind(parsed.parsed).bind(parsed.preview_png.is_some()).bind(uploaded_by).bind(now).fetch_one(&mut *tx).await?;
        let file = row_file(row)?;
        sqlx::query("DELETE FROM library_cleanup WHERE sha256=$1")
            .bind(sha)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok((file, true))
    }
    pub async fn get(&self, id: i64) -> Result<Option<LibraryFile>, StoreError> {
        sqlx::query("SELECT * FROM library_files WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .map(row_file)
            .transpose()
    }
    pub async fn list(
        &self,
        q: Option<&str>,
        sort: LibrarySort,
    ) -> Result<Vec<LibraryFile>, StoreError> {
        let order = match sort {
            LibrarySort::Newest => "uploaded_at DESC, id DESC",
            LibrarySort::Name => "name, version DESC",
            LibrarySort::SlicedFor => "meta->>'printer_model', name, version DESC",
        };
        let sql = format!(
            "SELECT * FROM library_files WHERE ($1::text IS NULL OR name ILIKE '%' || $1 || '%') ORDER BY {order}"
        );
        sqlx::query(&sql)
            .bind(q)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(row_file)
            .collect()
    }
    pub async fn reconcile(&self) -> Result<(), StoreError> {
        self.blobs.reconcile(&self.pool).await
    }
    pub async fn delete(&self, id: i64) -> Result<bool, StoreError> {
        let Some(file) = self.get(id).await? else {
            return Ok(false);
        };
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(1, hashtext($1))")
            .bind(&file.name)
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT pg_advisory_xact_lock(2, hashtext($1))")
            .bind(&file.sha256)
            .execute(&mut *tx)
            .await?;
        let result = sqlx::query("DELETE FROM library_files WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if result.rows_affected() == 0 {
            return Ok(false);
        }
        sqlx::query("INSERT INTO library_cleanup (sha256) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(&file.sha256)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.blobs.cleanup_digest(&self.pool, &file.sha256).await?;
        Ok(true)
    }
}
fn row_file(row: sqlx::postgres::PgRow) -> Result<LibraryFile, StoreError> {
    let kind: String = row.try_get("kind")?;
    let meta: Option<serde_json::Value> = row.try_get("meta")?;
    Ok(LibraryFile {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        version: row.try_get("version")?,
        sha256: row.try_get("sha256")?,
        size: row.try_get::<i64, _>("size")? as u64,
        kind: if kind == "bgcode" {
            LibraryKind::Bgcode
        } else {
            LibraryKind::Gcode
        },
        meta: meta
            .map(serde_json::from_value)
            .transpose()
            .map_err(io::Error::other)?,
        parsed: row.try_get("parsed")?,
        has_preview: row.try_get("has_preview")?,
        uploaded_by: row.try_get("uploaded_by")?,
        uploaded_at: row.try_get("uploaded_at")?,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_db::TestSchema;
    use futures_util::stream;
    use sha2::{Digest, Sha256};
    use starstar_shared::LibraryKind;

    #[tokio::test]
    async fn add_stores_blob_and_reuses_latest() {
        let schema = TestSchema::create().await;
        let db = schema.connect().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = LibraryStore::open(dir.path(), db.pool().clone()).unwrap();
        let data = include_bytes!("../../../../prints/cube-12mm/cube-12mm.MK4.bgcode");
        let upload = || stream::iter(vec![Ok::<_, ()>(axum::body::Bytes::from_static(data))]);
        let (file, created) = store
            .add(
                "cube.bgcode",
                LibraryKind::Bgcode,
                "test",
                upload(),
                data.len() as u64,
                200_000,
            )
            .await
            .unwrap();
        assert!(created && file.parsed && file.has_preview);
        assert!(store.blob_path(&file).exists());
        assert!(store.preview_path(&file).unwrap().exists());
        let (again, created) = store
            .add(
                "cube.bgcode",
                LibraryKind::Bgcode,
                "test",
                upload(),
                data.len() as u64,
                200_000,
            )
            .await
            .unwrap();
        assert!(!created);
        assert_eq!(file.id, again.id);
    }
    #[tokio::test]
    async fn aborted_and_concurrent_operations() {
        let schema = TestSchema::create().await;
        let db = schema.connect().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = LibraryStore::open(dir.path(), db.pool().clone()).unwrap();
        let bad = stream::iter(vec![
            Ok::<_, ()>(Bytes::from_static(b"1234567890")),
            Err(()),
        ]);
        assert!(matches!(
            store
                .add("bad.gcode", LibraryKind::Gcode, "test", bad, 100, 100)
                .await,
            Err(LibraryError::Incomplete)
        ));
        assert_eq!(
            std::fs::read_dir(dir.path().join("tmp")).unwrap().count(),
            0
        );
        let data = Bytes::from_static(b"G28\n");
        let make = || stream::iter(vec![Ok::<_, ()>(data.clone())]);
        let (a, b) = tokio::join!(
            store.add("a.gcode", LibraryKind::Gcode, "test", make(), 4, 100),
            store.add("b.gcode", LibraryKind::Gcode, "test", make(), 4, 100)
        );
        let a = a.unwrap().0;
        let b = b.unwrap().0;
        assert!(store.delete(a.id).await.unwrap());
        assert!(store.blob_path(&b).exists());
        let (deleted, added) = tokio::join!(
            store.delete(b.id),
            store.add("c.gcode", LibraryKind::Gcode, "test", make(), 4, 100)
        );
        assert!(deleted.unwrap());
        let c = added.unwrap().0;
        assert!(store.blob_path(&c).exists());
        assert!(store.delete(c.id).await.unwrap());
        assert!(!store.blob_path(&c).exists());
    }
    #[tokio::test]
    async fn concurrent_versions_and_search() {
        let schema = TestSchema::create().await;
        let db = schema.connect().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = LibraryStore::open(dir.path(), db.pool().clone()).unwrap();
        let body =
            |bytes: &'static [u8]| stream::iter(vec![Ok::<_, ()>(Bytes::from_static(bytes))]);
        let (a, b) = tokio::join!(
            store.add(
                "cube.gcode",
                LibraryKind::Gcode,
                "test",
                body(b"G28"),
                3,
                10
            ),
            store.add(
                "cube.gcode",
                LibraryKind::Gcode,
                "test",
                body(b"G29"),
                3,
                10
            )
        );
        let mut versions = [a.unwrap().0.version, b.unwrap().0.version];
        versions.sort();
        assert_eq!(versions, [1, 2]);
        let rows = store.list(Some("cub"), LibrarySort::Newest).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].id > rows[1].id);
        assert_eq!(
            store
                .list(Some("absent"), LibrarySort::Name)
                .await
                .unwrap()
                .len(),
            0
        );
        assert!(matches!(
            store
                .add(
                    "big.gcode",
                    LibraryKind::Gcode,
                    "test",
                    body(b"12345"),
                    5,
                    4
                )
                .await,
            Err(LibraryError::TooLarge(4))
        ));
        assert_eq!(
            std::fs::read_dir(dir.path().join("tmp")).unwrap().count(),
            0
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn open_refuses_unwritable_dir() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/test")
            .unwrap();
        let error = match LibraryStore::open(dir.path(), pool) {
            Ok(_) => panic!("expected failure"),
            Err(e) => e,
        };
        assert!(error.to_string().contains("STARSTAR_LIBRARY_DIR"));
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[tokio::test]
    async fn rejected_streams_leave_no_assets_or_rows() {
        let schema = TestSchema::create().await;
        let db = schema.connect().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = LibraryStore::open(dir.path(), db.pool().clone()).unwrap();
        let stream = || stream::iter(vec![Ok::<_, ()>(Bytes::from_static(b"12345"))]);
        assert!(matches!(
            store
                .add("short.gcode", LibraryKind::Gcode, "u", stream(), 10, 10)
                .await,
            Err(LibraryError::Incomplete)
        ));
        assert!(matches!(
            store
                .add("large.gcode", LibraryKind::Gcode, "u", stream(), 4, 4)
                .await,
            Err(LibraryError::TooLarge(4))
        ));
        assert!(
            store
                .list(None, LibrarySort::Newest)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            std::fs::read_dir(dir.path().join("tmp")).unwrap().count(),
            0
        );
        assert_eq!(
            std::fs::read_dir(dir.path().join("blobs")).unwrap().count(),
            0
        );
    }
    #[tokio::test]
    async fn shared_preview_and_cleanup_recovery() {
        let schema = TestSchema::create().await;
        let db = schema.connect().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = LibraryStore::open(dir.path(), db.pool().clone()).unwrap();
        let data = include_bytes!("../../../../prints/cube-12mm/cube-12mm.MK4.bgcode");
        let stream = || stream::iter(vec![Ok::<_, ()>(Bytes::from_static(data))]);
        let a = store
            .add(
                "a.bgcode",
                LibraryKind::Bgcode,
                "u",
                stream(),
                data.len() as u64,
                200_000,
            )
            .await
            .unwrap()
            .0;
        let b = store
            .add(
                "b.bgcode",
                LibraryKind::Bgcode,
                "u",
                stream(),
                data.len() as u64,
                200_000,
            )
            .await
            .unwrap()
            .0;
        let blob = store.blob_path(&a);
        let preview = store.preview_path(&a).unwrap();
        store.reconcile().await.unwrap();
        assert!(blob.exists() && preview.exists());
        store.delete(a.id).await.unwrap();
        assert!(blob.exists() && preview.exists());
        // Force cleanup to fail after the catalog deletion; its intent survives.
        std::fs::remove_file(&preview).unwrap();
        std::fs::create_dir(&preview).unwrap();
        assert!(store.delete(b.id).await.is_err());
        assert!(store.get(b.id).await.unwrap().is_none());
        let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM library_cleanup")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(pending, 1);
        std::fs::remove_dir(&preview).unwrap();
        store.reconcile().await.unwrap();
        assert!(!blob.exists() && !preview.exists());
        let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM library_cleanup")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(pending, 0);
        // Simulate publication interrupted before its catalog transaction commits.
        std::fs::write(&blob, b"orphan").unwrap();
        std::fs::write(&preview, b"orphan").unwrap();
        store.reconcile().await.unwrap();
        assert!(!blob.exists() && !preview.exists());
    }
    #[tokio::test]
    async fn last_reference_removes_real_blob_and_preview() {
        let schema = TestSchema::create().await;
        let db = schema.connect().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = LibraryStore::open(dir.path(), db.pool().clone()).unwrap();
        let data = include_bytes!("../../../../prints/cube-12mm/cube-12mm.MK4.bgcode");
        let body = stream::iter(vec![Ok::<_, ()>(Bytes::from_static(data))]);
        let file = store
            .add(
                "cube.bgcode",
                LibraryKind::Bgcode,
                "u",
                body,
                data.len() as u64,
                200_000,
            )
            .await
            .unwrap()
            .0;
        let blob = store.blob_path(&file);
        let preview = store.preview_path(&file).unwrap();
        assert!(blob.is_file() && preview.is_file());
        assert!(store.delete(file.id).await.unwrap());
        assert!(!blob.exists() && !preview.exists());
    }
    #[tokio::test]
    async fn reconcile_continues_after_obstructed_orphan() {
        let schema = TestSchema::create().await;
        let db = schema.connect().await.unwrap();
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("nested/a/library");
        let store = LibraryStore::open(&dir, db.pool().clone()).unwrap();
        let data = b"G28\n";
        let bad = format!("{:x}", Sha256::digest(data));
        let good = "f".repeat(64);
        let obstructed = dir.join("thumbs").join(format!("{bad}.png"));
        std::fs::create_dir(&obstructed).unwrap();
        let orphan = dir.join("thumbs").join(format!("{good}.png"));
        std::fs::write(&orphan, b"orphan").unwrap();
        store.reconcile().await.unwrap();
        assert!(obstructed.is_dir());
        assert!(!orphan.exists());
        let pending: Vec<String> = sqlx::query_scalar("SELECT sha256 FROM library_cleanup")
            .fetch_all(db.pool())
            .await
            .unwrap();
        assert_eq!(pending, vec![bad.clone()]);
        let upload = stream::iter(vec![Ok::<_, ()>(Bytes::from_static(data))]);
        assert!(
            store
                .add("bad.gcode", LibraryKind::Gcode, "u", upload, 4, 100)
                .await
                .is_err()
        );
        assert!(
            store
                .list(None, LibrarySort::Newest)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(!store.blobs.path(&bad).exists());
        assert_eq!(std::fs::read_dir(dir.join("tmp")).unwrap().count(), 0);
        std::fs::remove_dir(obstructed).unwrap();
        let blob = store.blobs.path(&bad);
        std::fs::create_dir_all(&blob).unwrap();
        let upload = stream::iter(vec![Ok::<_, ()>(Bytes::from_static(data))]);
        assert!(
            store
                .add("bad.gcode", LibraryKind::Gcode, "u", upload, 4, 100)
                .await
                .is_err()
        );
        assert!(
            store
                .list(None, LibrarySort::Newest)
                .await
                .unwrap()
                .is_empty()
        );
        std::fs::remove_dir(&blob).unwrap();
        store.reconcile().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM library_cleanup")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
