use crate::{
    AppState,
    auth::{Admin, Operator, Viewer},
    error::ApiError,
    library,
};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;
use starstar_shared::{LibraryFile, LibraryQuery};
use tokio_util::io::ReaderStream;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/library", get(list).post(upload))
        .route("/api/library/{id}", get(detail).delete(delete))
        .route("/api/library/{id}/preview", get(preview))
        .route("/api/library/{id}/download", get(download))
}
async fn list(
    Viewer(c): Viewer,
    State(s): State<AppState>,
    Query(q): Query<LibraryQuery>,
) -> Result<Json<Vec<LibraryFile>>, ApiError> {
    Ok(Json(library::list_files(&s, &c, q).await?))
}
#[derive(Deserialize)]
struct UploadQuery {
    name: String,
}
async fn upload(
    Operator(c): Operator,
    State(s): State<AppState>,
    Query(q): Query<UploadQuery>,
    headers: HeaderMap,
    body: Body,
) -> Result<(StatusCode, Json<LibraryFile>), ApiError> {
    let len = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse().ok())
        .ok_or_else(|| ApiError::LengthRequired("uploads need a Content-Length header".into()))?;
    let (file, created) = library::add_file(&s, &c, &q.name, body.into_data_stream(), len).await?;
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(file),
    ))
}
async fn detail(
    Viewer(c): Viewer,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<LibraryFile>, ApiError> {
    Ok(Json(library::get_file(&s, &c, id).await?))
}
async fn preview(
    Viewer(c): Viewer,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    let bytes = library::preview(&s, &c, id).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "private, max-age=300"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; sandbox; frame-ancestors 'none'",
            ),
        ],
        bytes,
    )
        .into_response())
}
async fn download(
    Viewer(c): Viewer,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    let (file, handle) = library::open_download(&s, &c, id).await?;
    // RFC 5987 attr-char permits these punctuation characters without escaping.
    let encoded = utf8_percent_encode(&file.name, NON_ALPHANUMERIC)
        .to_string()
        .replace("%2D", "-")
        .replace("%2E", ".")
        .replace("%5F", "_")
        .replace("%7E", "~");
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename*=UTF-8''{encoded}"),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
        ],
        Body::from_stream(ReaderStream::new(handle)),
    )
        .into_response())
}
async fn delete(
    Admin(c): Admin,
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    library::delete_file(&s, &c, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
