use crate::store::StoreError;
use axum::body::Bytes;
use futures_util::{Stream, StreamExt};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use starstar_shared::{LibraryFile, LibraryKind, LibrarySort};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;

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
    dir: PathBuf,
    pool: PgPool,
    _temp: Option<tempfile::TempDir>,
}
impl LibraryStore {
    pub fn open(dir: &Path, pool: PgPool) -> Result<Self, StoreError> {
        create_durable_dirs(dir)?;
        for sub in ["blobs", "thumbs", "tmp"] {
            create_durable_dirs(&dir.join(sub))?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if std::fs::metadata(dir)?.permissions().mode() & 0o222 == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "directory is not writable",
                )
                .into());
            }
        }
        for sub in ["blobs", "thumbs", "tmp"] {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if std::fs::metadata(dir.join(sub))?.permissions().mode() & 0o222 == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        format!("{sub} is not writable"),
                    )
                    .into());
                }
            }
            drop(tempfile::NamedTempFile::new_in(dir.join(sub))?);
        }
        for entry in std::fs::read_dir(dir.join("tmp"))? {
            let path = entry?.path();
            if path.is_dir() {
                std::fs::remove_dir_all(path)?;
            } else {
                std::fs::remove_file(path)?;
            }
        }
        sync_dir(dir)?;
        Ok(Self {
            dir: dir.to_owned(),
            pool,
            _temp: None,
        })
    }
    pub fn temporary(pool: PgPool) -> Result<Self, StoreError> {
        let temp = tempfile::tempdir()?;
        let mut store = Self::open(temp.path(), pool)?;
        store._temp = Some(temp);
        Ok(store)
    }
    fn blob_for(&self, sha: &str) -> PathBuf {
        self.dir
            .join("blobs")
            .join(&sha[..2])
            .join(&sha[2..4])
            .join(sha)
    }
    pub fn blob_path(&self, f: &LibraryFile) -> PathBuf {
        self.blob_for(&f.sha256)
    }
    pub fn preview_path(&self, f: &LibraryFile) -> Option<PathBuf> {
        f.has_preview
            .then(|| self.dir.join("thumbs").join(format!("{}.png", f.sha256)))
    }
    pub async fn add<E>(
        &self,
        name: &str,
        kind: LibraryKind,
        uploaded_by: &str,
        mut body: impl Stream<Item = Result<Bytes, E>> + Unpin,
        expected_len: u64,
        max: u64,
    ) -> Result<(LibraryFile, bool), LibraryError> {
        if expected_len > max {
            return Err(LibraryError::TooLarge(max));
        }
        let temp = tempfile::NamedTempFile::new_in(self.dir.join("tmp"))?;
        let mut out = tokio::fs::File::from_std(temp.reopen()?);
        let mut hash = Sha256::new();
        let mut size = 0u64;
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(|_| LibraryError::Incomplete)?;
            size = size
                .checked_add(chunk.len() as u64)
                .ok_or(LibraryError::TooLarge(max))?;
            if size > max {
                return Err(LibraryError::TooLarge(max));
            }
            hash.update(&chunk);
            out.write_all(&chunk).await?;
        }
        if size != expected_len {
            return Err(LibraryError::Incomplete);
        }
        out.sync_all().await?;
        drop(out);
        let sha = format!("{:x}", hash.finalize());
        let parsed = super::parse::parse_file(temp.path(), kind)?;
        let mut tx = self.pool.begin().await?;
        // Separate namespaces prevent name/digest lock collisions. All adds acquire name first.
        sqlx::query("SELECT pg_advisory_xact_lock(1, hashtext($1))")
            .bind(name)
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT pg_advisory_xact_lock(2, hashtext($1))")
            .bind(&sha)
            .execute(&mut *tx)
            .await?;
        let blob = self.blob_for(&sha);
        let preview = self.dir.join("thumbs").join(format!("{sha}.png"));
        let blob_exists = regular_or_absent(&blob)?;
        let preview_exists = regular_or_absent(&preview)?;
        if let Some(row) =
            sqlx::query("SELECT * FROM library_files WHERE name = $1 ORDER BY version DESC LIMIT 1")
                .bind(name)
                .fetch_optional(&mut *tx)
                .await?
        {
            let existing = row_file(row)?;
            if existing.sha256 == sha {
                tx.commit().await?;
                return Ok((existing, false));
            }
        }
        let blob_parent = blob.parent().expect("blob parent");
        std::fs::create_dir_all(blob_parent)?;
        sync_dir(&self.dir.join("blobs"))?;
        sync_dir(blob_parent.parent().expect("blob shard parent"))?;
        if !blob_exists {
            std::fs::rename(temp.path(), &blob)?;
        }
        // Sync existing files too: a previous interrupted publication may have
        // left an entry whose directory was never synced.
        std::fs::File::open(&blob)?.sync_all()?;
        sync_dir(blob_parent)?;
        sync_dir(&self.dir.join("tmp"))?;
        if let Some(bytes) = &parsed.preview_png
            && !preview_exists
        {
            use std::io::Write;
            let mut preview_temp = tempfile::NamedTempFile::new_in(self.dir.join("tmp"))?;
            preview_temp.write_all(bytes)?;
            preview_temp.as_file().sync_all()?;
            preview_temp
                .persist(&preview)
                .map_err(|error| error.error)?;
        }
        if preview.exists() {
            std::fs::File::open(&preview)?.sync_all()?;
            sync_dir(&self.dir.join("thumbs"))?;
            sync_dir(&self.dir.join("tmp"))?;
        }
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
            .bind(name).bind(version).bind(&sha).bind(size as i64).bind(match kind { LibraryKind::Bgcode => "bgcode", LibraryKind::Gcode => "gcode" }).bind(meta).bind(parsed.parsed).bind(parsed.preview_png.is_some()).bind(uploaded_by).bind(now).fetch_one(&mut *tx).await?;
        let file = row_file(row)?;
        sqlx::query("DELETE FROM library_cleanup WHERE sha256=$1")
            .bind(&sha)
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
    /// Reconcile durable cleanup intents and assets left by interrupted uploads.
    /// Call once during startup, before accepting requests.
    pub async fn reconcile(&self) -> Result<(), StoreError> {
        let mut digests: std::collections::BTreeSet<String> =
            sqlx::query_scalar::<_, String>("SELECT sha256 FROM library_cleanup")
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .collect();
        for entry in std::fs::read_dir(self.dir.join("thumbs"))? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(sha) = name.strip_suffix(".png").filter(|sha| valid_digest(sha)) {
                digests.insert(sha.to_owned());
            }
        }
        for first in std::fs::read_dir(self.dir.join("blobs"))? {
            let first = first?;
            if !first.file_type()?.is_dir() {
                continue;
            }
            for second in std::fs::read_dir(first.path())? {
                let second = second?;
                if !second.file_type()?.is_dir() {
                    continue;
                }
                for entry in std::fs::read_dir(second.path())? {
                    let entry = entry?;
                    if !entry.file_type()?.is_file() {
                        continue;
                    }
                    let name = entry.file_name();
                    let sha = name.to_string_lossy();
                    if valid_digest(&sha) && entry.path() == self.blob_for(&sha) {
                        digests.insert(sha.into_owned());
                    }
                }
            }
        }
        for sha in digests {
            if valid_digest(&sha) {
                // Persist the intent before attempting cleanup, including for
                // orphans discovered by scanning rather than by deletion.
                sqlx::query(
                    "INSERT INTO library_cleanup (sha256) VALUES ($1) ON CONFLICT DO NOTHING",
                )
                .bind(&sha)
                .execute(&self.pool)
                .await?;
                if let Err(error) = self.cleanup_digest(&sha).await {
                    eprintln!("library cleanup for {sha} failed: {error}");
                }
            }
        }
        Ok(())
    }
    async fn cleanup_digest(&self, sha: &str) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(2, hashtext($1))")
            .bind(sha)
            .execute(&mut *tx)
            .await?;
        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM library_files WHERE sha256=$1")
                .bind(sha)
                .fetch_one(&mut *tx)
                .await?;
        if remaining == 0 {
            let blob = self.blob_for(sha);
            let preview = self.dir.join("thumbs").join(format!("{sha}.png"));
            for path in [&blob, &preview] {
                if let Err(e) = std::fs::remove_file(path)
                    && e.kind() != io::ErrorKind::NotFound
                {
                    return Err(e.into());
                }
            }
            if let Some(parent) = blob.parent().filter(|p| p.exists()) {
                sync_dir(parent)?;
            }
            sync_dir(&self.dir.join("thumbs"))?;
        }
        sqlx::query("DELETE FROM library_cleanup WHERE sha256=$1")
            .bind(sha)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
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
        self.cleanup_digest(&file.sha256).await?;
        Ok(true)
    }
}
fn regular_or_absent(path: &Path) -> io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "library asset is not a regular file",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}
fn create_durable_dirs(path: &Path) -> io::Result<()> {
    let mut missing = Vec::new();
    let mut current = path;
    while !current.exists() {
        missing.push(current.to_path_buf());
        current = current
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
    }
    for directory in missing.iter().rev() {
        match std::fs::create_dir(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && directory.is_dir() => {}
            Err(error) => return Err(error),
        }
        let parent = directory
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        sync_dir(parent)?;
        sync_dir(directory)?;
    }
    // Also persist the entry for an already-existing configured directory.
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    sync_dir(parent)?;
    Ok(())
}
fn sync_dir(path: &Path) -> io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}
fn valid_digest(sha: &str) -> bool {
    sha.len() == 64
        && sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
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
        assert!(!store.blob_for(&bad).exists());
        assert_eq!(std::fs::read_dir(dir.join("tmp")).unwrap().count(), 0);
        std::fs::remove_dir(obstructed).unwrap();
        let blob = store.blob_for(&bad);
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
