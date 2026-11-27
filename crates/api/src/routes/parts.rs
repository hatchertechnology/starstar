use crate::{
    AppState,
    auth::{Admin, Operator, Viewer},
    error::ApiError,
    library::parts::{FileLevel, MatchPrinter, PartPatch, UploadSpec, match_printers},
};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::future::join_all;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;
use starstar_shared::{
    FileRole, ModelRevision, Part, PartFile, PartSort, PartSummary, PrinterMatch, Setup,
    SetupRevision,
};
use tokio_util::io::ReaderStream;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/parts", get(list).post(create))
        .route(
            "/api/parts/{id}",
            get(detail).patch(patch).delete(delete_part),
        )
        .route(
            "/api/parts/{id}/revisions",
            axum::routing::post(model_revision),
        )
        .route("/api/parts/{id}/setups", axum::routing::post(create_setup))
        .route("/api/parts/{id}/files", axum::routing::post(upload))
        .route("/api/parts/{id}/match", get(matches))
        .route(
            "/api/setups/{id}",
            axum::routing::patch(patch_setup).delete(delete_setup),
        )
        .route(
            "/api/setups/{id}/revisions",
            axum::routing::post(setup_revision),
        )
        .route(
            "/api/model-revisions/{id}",
            axum::routing::delete(delete_model_revision),
        )
        .route(
            "/api/setup-revisions/{id}",
            axum::routing::delete(delete_setup_revision),
        )
        .route(
            "/api/files/{id}",
            get(file).patch(patch_file).delete(delete_file),
        )
        .route("/api/files/{id}/preview", get(preview))
        .route("/api/files/{id}/download", get(download))
}
#[derive(Deserialize)]
struct ListQuery {
    q: Option<String>,
    sort: Option<PartSort>,
}
async fn list(
    _: Viewer,
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<PartSummary>>, ApiError> {
    Ok(Json(
        s.parts
            .list(q.q.as_deref(), q.sort.unwrap_or_default())
            .await?,
    ))
}
#[derive(Deserialize)]
struct Create {
    name: String,
    notes: Option<String>,
}
async fn create(
    Operator(c): Operator,
    State(s): State<AppState>,
    Json(q): Json<Create>,
) -> Result<(StatusCode, Json<Part>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(
            s.parts
                .create_part(&q.name, q.notes.as_deref(), &c.username)
                .await?,
        ),
    ))
}
async fn detail(
    _: Viewer,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Part>, ApiError> {
    Ok(Json(s.parts.tree(id).await?.ok_or_else(|| {
        ApiError::NotFound("part not found".into())
    })?))
}
fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(d).map(Some)
}
#[derive(Deserialize)]
struct PatchPart {
    name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    notes: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    default_setup_id: Option<Option<i64>>,
}
async fn patch(
    _: Operator,
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(q): Json<PatchPart>,
) -> Result<Json<Part>, ApiError> {
    Ok(Json(
        s.parts
            .update_part(
                id,
                PartPatch {
                    name: q.name,
                    notes: q.notes,
                    default_setup_id: q.default_setup_id,
                },
            )
            .await?,
    ))
}
async fn delete_part(
    _: Admin,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    s.parts.delete_part(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
struct Revision {
    note: Option<String>,
    #[serde(default)]
    carry: Vec<i64>,
}
async fn model_revision(
    Operator(c): Operator,
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(q): Json<Revision>,
) -> Result<(StatusCode, Json<ModelRevision>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(
            s.parts
                .new_model_revision(id, q.note.as_deref(), &q.carry, &c.username)
                .await?,
        ),
    ))
}
#[derive(Deserialize)]
struct Named {
    name: String,
}
async fn create_setup(
    Operator(c): Operator,
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(q): Json<Named>,
) -> Result<(StatusCode, Json<Setup>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(s.parts.create_setup(id, &q.name, &c.username).await?),
    ))
}
#[derive(Deserialize)]
struct SetupPatch {
    name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    default_profile: Option<Option<String>>,
}
async fn patch_setup(
    _: Admin,
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(q): Json<SetupPatch>,
) -> Result<Json<Setup>, ApiError> {
    Ok(Json(
        s.parts.update_setup(id, q.name, q.default_profile).await?,
    ))
}
async fn delete_setup(
    _: Admin,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    s.parts.delete_setup(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn delete_model_revision(
    _: Admin,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    s.parts.delete_model_revision(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn delete_setup_revision(
    _: Admin,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    s.parts.delete_setup_revision(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
struct SetupRevisionInput {
    model_revision: Option<i64>,
    note: Option<String>,
}
async fn setup_revision(
    Operator(c): Operator,
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(q): Json<SetupRevisionInput>,
) -> Result<(StatusCode, Json<SetupRevision>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(
            s.parts
                .new_setup_revision(id, q.model_revision, q.note.as_deref(), &c.username)
                .await?,
        ),
    ))
}
#[derive(Deserialize)]
struct Upload {
    name: String,
    role: Option<FileRole>,
    model_revision: Option<i64>,
    setup: Option<i64>,
    setup_revision: Option<i64>,
    profile: Option<String>,
    #[serde(default)]
    replace: bool,
}
async fn upload(
    Operator(c): Operator,
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<Upload>,
    headers: HeaderMap,
    body: Body,
) -> Result<(StatusCode, Json<PartFile>), ApiError> {
    let len = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse().ok())
        .ok_or_else(|| ApiError::LengthRequired("uploads need a Content-Length header".into()))?;
    let role = q
        .role
        .or_else(|| FileRole::infer(&q.name))
        .ok_or_else(|| ApiError::BadRequest("file role required".into()))?;
    let valid_extra =
        q.model_revision.is_some() && q.setup.is_some() && role == FileRole::Project3mf;
    if [
        q.model_revision.is_some(),
        q.setup.is_some(),
        q.setup_revision.is_some(),
    ]
    .into_iter()
    .filter(|x| *x)
    .count()
        > if valid_extra { 2 } else { 1 }
    {
        return Err(ApiError::BadRequest("choose one file level".into()));
    }
    let level = if let Some(id) = q.setup {
        FileLevel::Setup(id)
    } else if let Some(id) = q.setup_revision {
        FileLevel::SetupRevision(id)
    } else if let Some(id) = q.model_revision {
        FileLevel::ModelRevision(id)
    } else if matches!(role, FileRole::Source | FileRole::Render) {
        let part = s.parts.get_part(id).await?;
        FileLevel::ModelRevision(
            part.model_revisions
                .first()
                .ok_or_else(|| ApiError::NotFound("model revision not found".into()))?
                .id,
        )
    } else {
        FileLevel::Part
    };
    let spec = UploadSpec {
        part_id: id,
        model_revision_id: if valid_extra { q.model_revision } else { None },
        level,
        role,
        name: q.name,
        profile: q.profile,
        replace: q.replace,
    };
    let (f, created) = s
        .parts
        .add_file(
            spec,
            &c.username,
            body.into_data_stream(),
            len,
            s.config.max_upload_bytes,
        )
        .await?;
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(f),
    ))
}
async fn get_file(s: &AppState, id: i64) -> Result<PartFile, ApiError> {
    s.parts
        .get_file(id)
        .await?
        .ok_or_else(|| ApiError::NotFound("file not found".into()))
}
async fn file(
    _: Viewer,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<PartFile>, ApiError> {
    Ok(Json(get_file(&s, id).await?))
}
#[derive(Deserialize)]
struct FilePatch {
    #[serde(default, deserialize_with = "double_option")]
    profile: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    note: Option<Option<String>>,
}
async fn patch_file(
    _: Operator,
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(q): Json<FilePatch>,
) -> Result<Json<PartFile>, ApiError> {
    Ok(Json(s.parts.update_file(id, q.profile, q.note).await?))
}
async fn delete_file(
    _: Admin,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    s.parts.delete_file(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
fn secure_asset(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    response
}
async fn preview(
    _: Viewer,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    let f = get_file(&s, id).await?;
    if !f.has_preview || f.role == FileRole::Other {
        return Err(ApiError::NotFound("file has no preview".into()));
    }
    let data = tokio::fs::read(s.parts.blobs.preview(&f.sha256))
        .await
        .map_err(|_| ApiError::Internal("internal error".into()))?;
    Ok(secure_asset(
        (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "private, max-age=300"),
                (
                    header::CONTENT_SECURITY_POLICY,
                    "default-src 'none'; sandbox; frame-ancestors 'none'",
                ),
            ],
            data,
        )
            .into_response(),
    ))
}
async fn download(
    _: Viewer,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    let f = get_file(&s, id).await?;
    let handle = tokio::fs::File::open(s.parts.blobs.path(&f.sha256))
        .await
        .map_err(|_| ApiError::Internal("internal error".into()))?;
    let encoded = utf8_percent_encode(&f.name, NON_ALPHANUMERIC)
        .to_string()
        .replace("%2D", "-")
        .replace("%2E", ".")
        .replace("%5F", "_")
        .replace("%7E", "~");
    Ok(secure_asset(
        (
            [
                (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename*=UTF-8''{encoded}"),
                ),
            ],
            Body::from_stream(ReaderStream::new(handle)),
        )
            .into_response(),
    ))
}
#[derive(Deserialize)]
struct MatchQuery {
    setup: Option<i64>,
    profile: Option<String>,
}
async fn matches(
    _: Operator,
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<MatchQuery>,
) -> Result<Json<Vec<PrinterMatch>>, ApiError> {
    let part = s.parts.get_part(id).await?;
    let printers = s.store.list().await?;
    let results = join_all(
        printers
            .iter()
            .map(|p| async { crate::printers::client(&s, p).info().await }),
    )
    .await;
    let inputs: Vec<_> = printers
        .iter()
        .zip(&results)
        .map(|(p, r)| MatchPrinter {
            name: p.name.clone(),
            kind: p.kind,
            nozzle: r.as_ref().ok().and_then(|x| x.nozzle_diameter),
        })
        .collect();
    let mut matches = match_printers(&part, q.setup, q.profile.as_deref(), &inputs)?;
    for (m, r) in matches.iter_mut().zip(results) {
        if r.is_err() {
            m.blocked = Some("printer unreachable".into());
            m.slice = None;
        }
    }
    Ok(Json(matches))
}
