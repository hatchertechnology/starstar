use super::{parse, store::LibraryError};
use crate::store::StoreError;
use axum::body::Bytes;
use futures_util::{Stream, StreamExt};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use starstar_shared::LibraryKind;
use std::{
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
};
use tokio::io::AsyncWriteExt;

pub struct Blobs {
    dir: PathBuf,
    _temp: Option<tempfile::TempDir>,
}
pub enum PreviewSource {
    Slice(LibraryKind),
    Image,
    ThreeMf,
    None,
}
pub struct Ingested {
    pub sha256: String,
    pub size: u64,
    pub parsed: Option<parse::Parsed>,
    pub has_preview: bool,
    temp: tempfile::NamedTempFile,
    preview_png: Option<Vec<u8>>,
}
impl Blobs {
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
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
            _temp: None,
        })
    }
    pub fn temporary() -> Result<Self, StoreError> {
        let temp = tempfile::tempdir()?;
        let mut store = Self::open(temp.path())?;
        store._temp = Some(temp);
        Ok(store)
    }
    pub fn path(&self, sha: &str) -> PathBuf {
        self.dir
            .join("blobs")
            .join(&sha[..2])
            .join(&sha[2..4])
            .join(sha)
    }
    pub fn preview(&self, sha: &str) -> PathBuf {
        self.dir.join("thumbs").join(format!("{sha}.png"))
    }
    pub async fn receive<E>(
        &self,
        mut body: impl Stream<Item = Result<Bytes, E>> + Unpin,
        expected_len: u64,
        max: u64,
        source: PreviewSource,
    ) -> Result<Ingested, LibraryError> {
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
        let sha256 = format!("{:x}", hash.finalize());
        let parsed = match source {
            PreviewSource::Slice(kind) => Some(parse::parse_file(temp.path(), kind)?),
            _ => None,
        };
        let preview_png = match source {
            PreviewSource::Slice(_) => parsed.as_ref().and_then(|p| p.preview_png.clone()),
            PreviewSource::Image => image_preview(temp.path()),
            PreviewSource::ThreeMf => threemf_preview(temp.path()),
            PreviewSource::None => None,
        };
        Ok(Ingested {
            sha256,
            size,
            has_preview: preview_png.is_some(),
            parsed,
            temp,
            preview_png,
        })
    }
    pub(crate) fn validate_assets(&self, sha: &str) -> io::Result<()> {
        regular_or_absent(&self.path(sha))?;
        regular_or_absent(&self.preview(sha))?;
        Ok(())
    }
    pub fn publish(&self, ingested: &Ingested) -> io::Result<()> {
        let blob = self.path(&ingested.sha256);
        let preview = self.preview(&ingested.sha256);
        let blob_exists = regular_or_absent(&blob)?;
        let preview_exists = regular_or_absent(&preview)?;
        let blob_parent = blob.parent().expect("blob parent");
        std::fs::create_dir_all(blob_parent)?;
        sync_dir(&self.dir.join("blobs"))?;
        sync_dir(blob_parent.parent().expect("blob shard parent"))?;
        if !blob_exists {
            std::fs::rename(ingested.temp.path(), &blob)?;
        }
        // Sync existing files too: a previous interrupted publication may have
        // left an entry whose directory was never synced.
        std::fs::File::open(&blob)?.sync_all()?;
        sync_dir(blob_parent)?;
        sync_dir(&self.dir.join("tmp"))?;
        if let Some(bytes) = &ingested.preview_png
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
        Ok(())
    }
    /// Reconcile durable cleanup intents and assets left by interrupted uploads.
    /// Call once during startup, before accepting requests.
    pub async fn reconcile(&self, pool: &PgPool) -> Result<(), StoreError> {
        let mut digests: std::collections::BTreeSet<String> =
            sqlx::query_scalar::<_, String>("SELECT sha256 FROM library_cleanup")
                .fetch_all(pool)
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
                    if valid_digest(&sha) && entry.path() == self.path(&sha) {
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
                .execute(pool)
                .await?;
                if let Err(error) = self.cleanup_digest(pool, &sha).await {
                    eprintln!("library cleanup for {sha} failed: {error}");
                }
            }
        }
        Ok(())
    }
    pub async fn cleanup_digest(&self, pool: &PgPool, sha: &str) -> Result<(), StoreError> {
        let mut tx = pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(2, hashtext($1))")
            .bind(sha)
            .execute(&mut *tx)
            .await?;
        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM part_files WHERE sha256=$1")
            .bind(sha)
            .fetch_one(&mut *tx)
            .await?;
        if remaining == 0 {
            let blob = self.path(sha);
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
}
const MAX_PREVIEW_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
fn preview_source_allowed(size: u64) -> bool {
    size <= MAX_PREVIEW_SOURCE_BYTES
}
fn image_preview(path: &Path) -> Option<Vec<u8>> {
    if !preview_source_allowed(std::fs::metadata(path).ok()?.len()) {
        return None;
    }
    let mut reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    reader.limits(parse::image_limits(8192, 256 * 1024 * 1024));
    let img = reader.decode().ok()?;
    let img = if img.width() > 1024 || img.height() > 1024 {
        img.thumbnail(1024, 1024)
    } else {
        img
    };
    let mut output = Cursor::new(Vec::new());
    img.write_to(&mut output, image::ImageFormat::Png).ok()?;
    Some(output.into_inner())
}
fn threemf_preview(path: &Path) -> Option<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).ok()?).ok()?;
    let file = zip.by_name("Metadata/thumbnail.png").ok()?;
    if !preview_source_allowed(file.size()) || file.size() > 16 * 1024 * 1024 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 16 * 1024 * 1024 {
        return None;
    }
    parse::validated_png(&bytes)
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Bytes;
    use futures_util::stream;
    use std::io::{Cursor, Write};

    async fn ingest(data: Vec<u8>, source: PreviewSource) -> (Blobs, Ingested) {
        let blobs = Blobs::temporary().unwrap();
        let len = data.len() as u64;
        let item = blobs
            .receive(
                stream::iter(vec![Ok::<_, ()>(Bytes::from(data))]),
                len,
                len,
                source,
            )
            .await
            .unwrap();
        blobs.publish(&item).unwrap();
        (blobs, item)
    }
    fn png() -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(8, 8)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }
    #[test]
    fn preview_source_size_bound() {
        assert!(preview_source_allowed(64 * 1024 * 1024));
        assert!(!preview_source_allowed(64 * 1024 * 1024 + 1));
    }
    #[tokio::test]
    async fn image_preview_is_png() {
        let mut jpeg = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(8, 8)
            .write_to(&mut jpeg, image::ImageFormat::Jpeg)
            .unwrap();
        let (blobs, item) = ingest(jpeg.into_inner(), PreviewSource::Image).await;
        assert!(item.has_preview);
        assert!(
            std::fs::read(blobs.preview(&item.sha256))
                .unwrap()
                .starts_with(b"\x89PNG\r\n\x1a\n")
        );
        let preview = image::open(blobs.preview(&item.sha256)).unwrap();
        assert_eq!((preview.width(), preview.height()), (8, 8));
    }
    fn archive(thumbnail: bool) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(
            if thumbnail {
                "Metadata/thumbnail.png"
            } else {
                "other"
            },
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(&png()).unwrap();
        zip.finish().unwrap().into_inner()
    }
    #[tokio::test]
    async fn threemf_thumbnail_extracted() {
        let (blobs, item) = ingest(archive(true), PreviewSource::ThreeMf).await;
        assert!(item.has_preview);
        assert_eq!(std::fs::read(blobs.preview(&item.sha256)).unwrap(), png());
    }
    #[tokio::test]
    async fn threemf_without_thumbnail_has_no_preview() {
        let (_, item) = ingest(archive(false), PreviewSource::ThreeMf).await;
        assert!(!item.has_preview);
    }
}
