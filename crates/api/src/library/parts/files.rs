use super::catalog::{PartsError, PartsStore, lock, missing, now};
use crate::library::blobs::PreviewSource;
use axum::body::Bytes;
use futures_util::Stream;
use sqlx::Row;
use starstar_shared::{FileRole, LibraryKind, PartFile, PrinterFamily};

pub enum FileLevel {
    Part,
    ModelRevision(i64),
    LatestModelRevision,
    Setup(i64),
    SetupRevision(i64),
}
pub struct UploadSpec {
    pub part_id: i64,
    pub model_revision_id: Option<i64>,
    pub level: FileLevel,
    pub role: FileRole,
    pub name: String,
    pub profile: Option<String>,
    pub replace: bool,
}
impl PartsStore {
    pub async fn add_file<E>(
        &self,
        spec: UploadSpec,
        by: &str,
        body: impl Stream<Item = Result<Bytes, E>> + Unpin,
        len: u64,
        max: u64,
    ) -> Result<(PartFile, bool), PartsError> {
        let accepted = match spec.role {
            FileRole::Source => ".step, .stp, .stl, .f3d, .scad, .py",
            FileRole::Render => ".png, .jpg, .jpeg, .webp",
            FileRole::Project3mf => ".3mf",
            FileRole::Slice => ".bgcode, .gcode",
            FileRole::Other => "any",
        };
        if !spec.role.accepts(&spec.name) {
            return Err(PartsError::BadRequest(format!(
                "accepted extensions: {accepted}"
            )));
        }
        let valid = matches!(
            (&spec.role, &spec.level),
            (
                FileRole::Source | FileRole::Render,
                FileLevel::ModelRevision(_) | FileLevel::LatestModelRevision
            ) | (
                FileRole::Other,
                FileLevel::Part | FileLevel::ModelRevision(_) | FileLevel::LatestModelRevision
            ) | (
                FileRole::Slice | FileRole::Project3mf,
                FileLevel::Setup(_) | FileLevel::SetupRevision(_)
            )
        );
        if !valid {
            return Err(PartsError::BadRequest(
                "file role does not match level".into(),
            ));
        }
        let ext = std::path::Path::new(&spec.name)
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let source = match spec.role {
            FileRole::Slice => PreviewSource::Slice(if ext == "bgcode" {
                LibraryKind::Bgcode
            } else {
                LibraryKind::Gcode
            }),
            FileRole::Render => PreviewSource::Image,
            FileRole::Project3mf => PreviewSource::ThreeMf,
            _ => PreviewSource::None,
        };
        let item = self.blobs.receive(body, len, max, source).await?;
        let mut tx = self.pool.begin().await?;
        lock(&mut tx, spec.part_id).await?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM parts WHERE id=$1)")
            .bind(spec.part_id)
            .fetch_one(&mut *tx)
            .await?;
        if !exists {
            return Err(missing("part"));
        }
        if let Some(id) = spec.model_revision_id {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM model_revisions WHERE id=$1 AND part_id=$2)",
            )
            .bind(id)
            .bind(spec.part_id)
            .fetch_one(&mut *tx)
            .await?;
            if !exists {
                return Err(missing("model revision"));
            }
        }
        let mut model = None;
        let mut revision = None;
        let mut setup_id = None;
        match spec.level {
            FileLevel::Part => {}
            FileLevel::ModelRevision(id) => {
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM model_revisions WHERE id=$1 AND part_id=$2)",
                )
                .bind(id)
                .bind(spec.part_id)
                .fetch_one(&mut *tx)
                .await?;
                if !exists {
                    return Err(missing("model revision"));
                }
                model = Some(id);
            }
            FileLevel::LatestModelRevision => {
                model = Some(sqlx::query_scalar("SELECT id FROM model_revisions WHERE part_id=$1 ORDER BY number DESC LIMIT 1")
                    .bind(spec.part_id)
                    .fetch_optional(&mut *tx)
                    .await?
                    .ok_or_else(|| missing("model revision"))?);
            }
            FileLevel::SetupRevision(id) => {
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM setup_revisions WHERE id=$1 AND part_id=$2)",
                )
                .bind(id)
                .bind(spec.part_id)
                .fetch_one(&mut *tx)
                .await?;
                if !exists {
                    return Err(missing("setup revision"));
                }
                revision = Some(id);
            }
            FileLevel::Setup(id) => {
                let name: String =
                    sqlx::query_scalar("SELECT name FROM setups WHERE id=$1 AND part_id=$2")
                        .bind(id)
                        .bind(spec.part_id)
                        .fetch_optional(&mut *tx)
                        .await?
                        .ok_or_else(|| missing("setup"))?;
                revision=sqlx::query_scalar("SELECT id FROM setup_revisions WHERE setup_id=$1 AND part_id=$2 ORDER BY number DESC LIMIT 1").bind(id).bind(spec.part_id).fetch_optional(&mut *tx).await?;
                if revision.is_none() && spec.role == FileRole::Slice {
                    return Err(PartsError::BadRequest(format!(
                        "setup {name} has no revisions"
                    )));
                }
                setup_id = Some(id);
            }
        }
        sqlx::query("SELECT pg_advisory_xact_lock(2, hashtext($1))")
            .bind(&item.sha256)
            .execute(&mut *tx)
            .await?;
        self.blobs
            .validate_assets(&item.sha256)
            .map_err(crate::store::StoreError::from)?;
        let role = match spec.role {
            FileRole::Source => "source",
            FileRole::Render => "render",
            FileRole::Project3mf => "project_3mf",
            FileRole::Slice => "slice",
            FileRole::Other => "other",
        };
        let existing: Option<(i64, String)> = sqlx::query_as("SELECT id,sha256 FROM part_files WHERE part_id=$1 AND model_revision_id IS NOT DISTINCT FROM $2 AND setup_revision_id IS NOT DISTINCT FROM $3 AND role=$4 AND name=$5 AND superseded_by IS NULL ORDER BY id DESC LIMIT 1").bind(spec.part_id).bind(model).bind(revision).bind(role).bind(&spec.name).fetch_optional(&mut *tx).await?;
        if let Some((id, sha)) = existing
            && sha == item.sha256
        {
            let file = self
                .files(&mut tx, "SELECT * FROM part_files WHERE id=$1", id)
                .await?
                .remove(0);
            tx.commit().await?;
            return Ok((file, false));
        }
        if spec.role == FileRole::Project3mf {
            let occupied: bool = if let Some(revision) = revision {
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM part_files WHERE setup_revision_id=$1 AND role='project_3mf')").bind(revision).fetch_one(&mut *tx).await?
            } else {
                false
            };
            if let Some(setup) = setup_id {
                let model_id: i64 = if let Some(id) = spec.model_revision_id {
                    id
                } else {
                    sqlx::query_scalar("SELECT id FROM model_revisions WHERE part_id=$1 ORDER BY number DESC LIMIT 1").bind(spec.part_id).fetch_one(&mut *tx).await?
                };
                let revision_model: Option<i64> = if let Some(id) = revision {
                    sqlx::query_scalar("SELECT model_revision_id FROM setup_revisions WHERE id=$1")
                        .bind(id)
                        .fetch_optional(&mut *tx)
                        .await?
                } else {
                    None
                };
                if occupied || revision_model != Some(model_id) {
                    let number:i32=sqlx::query_scalar("UPDATE setups SET next_revision=next_revision+1 WHERE id=$1 RETURNING next_revision-1").bind(setup).fetch_one(&mut *tx).await?;
                    revision=Some(sqlx::query_scalar("INSERT INTO setup_revisions(part_id,setup_id,number,model_revision_id,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6) RETURNING id").bind(spec.part_id).bind(setup).bind(number).bind(model_id).bind(by).bind(now()).fetch_one(&mut *tx).await?);
                }
            } else if occupied {
                return Err(PartsError::Conflict(
                    "setup revision already has a 3MF".into(),
                ));
            }
        }
        let parsed = item.parsed.as_ref().is_some_and(|p| p.parsed);
        let meta = item
            .parsed
            .as_ref()
            .filter(|p| p.parsed)
            .map(|p| p.meta.clone());
        let family = meta
            .as_ref()
            .and_then(|m| m.printer_model.as_deref())
            .and_then(PrinterFamily::from_printer_model);
        let raw_nozzle = meta.as_ref().and_then(|m| m.nozzle_diameter);
        let nozzle: Option<f64> =
            sqlx::query_scalar("SELECT round($1::double precision::numeric,3)::double precision")
                .bind(raw_nozzle)
                .fetch_one(&mut *tx)
                .await?;
        let profile = if spec.role == FileRole::Slice {
            spec.profile
                .or_else(|| meta.as_ref().and_then(|m| m.print_settings_id.clone()))
                .and_then(normalize_profile)
        } else {
            None
        };
        let family_str = family.map(|f| match f {
            PrinterFamily::Mk4 => "mk4",
            PrinterFamily::Mini => "mini",
        });
        let mut old = None;
        if spec.role == FileRole::Slice
            && let (Some(f), Some(n)) = (family, nozzle)
        {
            let found = matching_slice(
                &mut tx,
                revision.unwrap(),
                family_str,
                Some(n),
                profile.as_deref(),
                None,
            )
            .await?;
            if let Some(id) = found {
                if !spec.replace {
                    let candidate = self
                        .files(&mut tx, "SELECT * FROM part_files WHERE id=$1", id)
                        .await?
                        .remove(0);
                    let row=sqlx::query("SELECT s.name,r.number FROM setup_revisions r JOIN setups s ON s.id=r.setup_id WHERE r.id=$1").bind(revision).fetch_one(&mut *tx).await?;
                    let name: String = row.try_get("name")?;
                    let number: i32 = row.try_get("number")?;
                    let fam = match f {
                        PrinterFamily::Mk4 => "MK4",
                        PrinterFamily::Mini => "MINI",
                    };
                    return Err(PartsError::Conflict(format!(
                        "{name} r{number} already has a slice for {fam} · {n} · {}: {} (id {})",
                        profile.as_deref().unwrap_or(""),
                        candidate.name,
                        candidate.id
                    )));
                }
                old = Some(id);
            }
        }
        // The partial unique index requires the old row to leave the current set before insertion.
        if let Some(id) = old {
            sqlx::query("UPDATE part_files SET superseded_by=id WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        self.blobs
            .publish(&item)
            .map_err(crate::store::StoreError::from)?;
        let id:i64=sqlx::query_scalar("INSERT INTO part_files(part_id,model_revision_id,setup_revision_id,role,name,sha256,size,kind,meta,parsed,has_preview,uploaded_by,uploaded_at,target_family,target_nozzle,profile) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) RETURNING id").bind(spec.part_id).bind(model).bind(revision).bind(role).bind(&spec.name).bind(&item.sha256).bind(item.size as i64).bind(&ext).bind(meta.map(sqlx::types::Json)).bind(parsed).bind(item.has_preview).bind(by).bind(now()).bind(family_str).bind(nozzle).bind(profile).fetch_one(&mut *tx).await?;
        if let Some(old) = old {
            sqlx::query("UPDATE part_files SET superseded_by=$1 WHERE id=$2")
                .bind(id)
                .bind(old)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("DELETE FROM library_cleanup WHERE sha256=$1")
            .bind(&item.sha256)
            .execute(&mut *tx)
            .await?;
        let file = self
            .files(&mut tx, "SELECT * FROM part_files WHERE id=$1", id)
            .await?
            .remove(0);
        tx.commit().await?;
        Ok((file, true))
    }
    pub async fn get_file(&self, id: i64) -> Result<Option<PartFile>, PartsError> {
        let mut conn = self.pool.acquire().await?;
        Ok(self
            .files(&mut conn, "SELECT * FROM part_files WHERE id=$1", id)
            .await?
            .into_iter()
            .next())
    }
    pub async fn update_file(
        &self,
        id: i64,
        profile: Option<Option<String>>,
        note: Option<Option<String>>,
    ) -> Result<PartFile, PartsError> {
        let mut tx = self.pool.begin().await?;
        let part: i64 = sqlx::query_scalar("SELECT part_id FROM part_files WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| missing("file"))?;
        lock(&mut tx, part).await?;
        let profile = profile.map(|p| p.and_then(normalize_profile));
        if let Some(new_profile) = &profile {
            let file = self
                .files(&mut tx, "SELECT * FROM part_files WHERE id=$1", id)
                .await?
                .into_iter()
                .next()
                .ok_or_else(|| missing("file"))?;
            if file.role != FileRole::Slice {
                return Err(PartsError::BadRequest(
                    "profile is only valid for slices".into(),
                ));
            }
            if file.superseded_by.is_none()
                && let (Some(revision), Some(target)) = (file.setup_revision_id, file.target)
                && let (Some(family), Some(nozzle)) = (target.family, target.nozzle)
            {
                let family = match family {
                    PrinterFamily::Mk4 => "mk4",
                    PrinterFamily::Mini => "mini",
                };
                if matching_slice(
                    &mut tx,
                    revision,
                    Some(family),
                    Some(nozzle),
                    new_profile.as_deref(),
                    Some(id),
                )
                .await?
                .is_some()
                {
                    return Err(PartsError::Conflict("slice target already exists".into()));
                }
            }
        }
        let n=sqlx::query("UPDATE part_files SET profile=CASE WHEN $2 THEN $3 ELSE profile END, note=CASE WHEN $4 THEN $5 ELSE note END WHERE id=$1").bind(id).bind(profile.is_some()).bind(profile.flatten()).bind(note.is_some()).bind(note.flatten()).execute(&mut *tx).await?.rows_affected();
        if n == 0 {
            return Err(missing("file"));
        }
        let file = self
            .files(&mut tx, "SELECT * FROM part_files WHERE id=$1", id)
            .await?
            .remove(0);
        tx.commit().await?;
        Ok(file)
    }
    pub async fn delete_file(&self, id: i64) -> Result<(), PartsError> {
        let mut tx = self.pool.begin().await?;
        let part: i64 = sqlx::query_scalar("SELECT part_id FROM part_files WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| missing("file"))?;
        lock(&mut tx, part).await?;
        let row = sqlx::query("SELECT sha256,superseded_by FROM part_files WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| missing("file"))?;
        let sha: String = row.try_get("sha256")?;
        let successor: Option<i64> = row.try_get("superseded_by")?;
        let pred: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM part_files WHERE superseded_by=$1 ORDER BY id DESC LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(pred) = pred {
            if successor.is_none() {
                let current: Option<i64> = sqlx::query_scalar("SELECT current.id FROM part_files predecessor JOIN part_files current ON current.setup_revision_id=predecessor.setup_revision_id AND current.role='slice' AND current.superseded_by IS NULL AND current.id<>$2 AND current.target_family=predecessor.target_family AND round(current.target_nozzle::numeric,3)=round(predecessor.target_nozzle::numeric,3) AND lower(coalesce(current.profile,''))=lower(coalesce(predecessor.profile,'')) WHERE predecessor.id=$1 LIMIT 1").bind(pred).bind(id).fetch_optional(&mut *tx).await?;
                if let Some(current) = current {
                    sqlx::query("UPDATE part_files SET superseded_by=$1 WHERE superseded_by=$2")
                        .bind(current)
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                } else {
                    sqlx::query("UPDATE part_files SET superseded_by=id WHERE id=$1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query("UPDATE part_files SET superseded_by=NULL WHERE id=$1")
                        .bind(pred)
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query(
                        "UPDATE part_files SET superseded_by=$1 WHERE superseded_by=$2 AND id<>$2",
                    )
                    .bind(pred)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                }
            } else {
                sqlx::query("UPDATE part_files SET superseded_by=$1 WHERE id=$2")
                    .bind(successor)
                    .bind(pred)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE part_files SET superseded_by=$1 WHERE superseded_by=$2")
                    .bind(pred)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        sqlx::query("INSERT INTO library_cleanup(sha256) VALUES($1) ON CONFLICT DO NOTHING")
            .bind(&sha)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM part_files WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.blobs.cleanup_digest(&self.pool, &sha).await?;
        Ok(())
    }
}

fn normalize_profile(profile: String) -> Option<String> {
    let trimmed = profile.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

async fn matching_slice(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    revision: i64,
    family: Option<&str>,
    nozzle: Option<f64>,
    profile: Option<&str>,
    exclude: Option<i64>,
) -> Result<Option<i64>, PartsError> {
    Ok(sqlx::query_scalar("SELECT id FROM part_files WHERE setup_revision_id=$1 AND role='slice' AND superseded_by IS NULL AND target_family=$2 AND round(target_nozzle::numeric,3)=round($3::double precision::numeric,3) AND lower(coalesce(profile,''))=lower(coalesce($4::text,'')) AND ($5::bigint IS NULL OR id<>$5) LIMIT 1").bind(revision).bind(family).bind(nozzle).bind(profile).bind(exclude).fetch_optional(&mut **tx).await?)
}
