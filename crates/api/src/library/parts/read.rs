use super::{PartsError, PartsStore};
use starstar_shared::{FileRole, Part, PartSort, PartSummary};

fn preview(part: &Part) -> Option<i64> {
    part.model_revisions
        .first()
        .and_then(|r| {
            r.files
                .iter()
                .filter(|f| f.role == FileRole::Render && f.has_preview)
                .max_by_key(|f| (f.uploaded_at, f.id))
        })
        .map(|f| f.id)
        .or_else(|| {
            let revision = part
                .setups
                .iter()
                .find(|s| Some(s.id) == part.default_setup_id)?
                .revisions
                .first()?;
            revision
                .project
                .as_ref()
                .filter(|f| f.has_preview)
                .or_else(|| revision.slices.iter().find(|f| f.has_preview))
                .map(|f| f.id)
        })
}
impl PartsStore {
    pub async fn tree(&self, id: i64) -> Result<Option<Part>, PartsError> {
        match self.get_part(id).await {
            Ok(mut part) => {
                part.preview_file_id = preview(&part);
                Ok(Some(part))
            }
            Err(PartsError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
    pub async fn list(
        &self,
        q: Option<&str>,
        sort: PartSort,
    ) -> Result<Vec<PartSummary>, PartsError> {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT p.id FROM parts p WHERE $1::text IS NULL OR p.name ILIKE '%' || $1 || '%' OR EXISTS (SELECT 1 FROM part_files f WHERE f.part_id=p.id AND (f.name ILIKE '%' || $1 || '%' OR f.profile ILIKE '%' || $1 || '%'))").bind(q).fetch_all(&self.pool).await?;
        let mut result = Vec::new();
        for id in ids {
            if let Some(p) = self.tree(id).await? {
                let files = p
                    .model_revisions
                    .iter()
                    .flat_map(|r| r.files.iter())
                    .chain(p.other_files.iter())
                    .chain(
                        p.setups
                            .iter()
                            .flat_map(|s| s.revisions.iter())
                            .flat_map(|r| r.project.iter().chain(r.slices.iter())),
                    );
                let updated_at = files
                    .map(|f| f.uploaded_at)
                    .chain(p.model_revisions.iter().map(|r| r.created_at))
                    .chain(
                        p.setups
                            .iter()
                            .flat_map(|s| s.revisions.iter().map(|r| r.created_at)),
                    )
                    .fold(p.created_at, i64::max);
                result.push(PartSummary {
                    id,
                    name: p.name,
                    latest_model_revision: p.model_revisions.first().map_or(0, |r| r.number),
                    setups: p.setups.len() as u32,
                    slices: p
                        .setups
                        .iter()
                        .flat_map(|s| s.revisions.iter())
                        .map(|r| {
                            r.slices
                                .iter()
                                .filter(|f| f.superseded_by.is_none())
                                .count() as u32
                        })
                        .sum(),
                    preview_file_id: p.preview_file_id,
                    updated_at,
                });
            }
        }
        match sort {
            PartSort::Name => result.sort_by_key(|p| (p.name.to_lowercase(), p.id)),
            PartSort::Newest => result.sort_by(|a, b| {
                b.updated_at
                    .cmp(&a.updated_at)
                    .then_with(|| b.id.cmp(&a.id))
            }),
        }
        Ok(result)
    }
}
