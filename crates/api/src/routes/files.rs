//! File routes. Paths after the storage name are captured whole by `{*path}`.

use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use starstar_shared::FileEntry;

use crate::AppState;
use crate::auth::Viewer;
use crate::error::ApiError;
use crate::printers::files;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/printers/{name}/files/{storage}", get(storage_root))
        .route(
            "/api/printers/{name}/files/{storage}/{*path}",
            // `.head` goes after `.get` so it replaces axum's implicit HEAD-from-GET.
            get(file_info).head(file_head),
        )
}

async fn storage_root(
    Viewer(caller): Viewer,
    State(state): State<AppState>,
    Path((name, storage)): Path<(String, String)>,
) -> Result<Json<FileEntry>, ApiError> {
    Ok(Json(
        files::file_info(&state, &caller, &name, &storage, None).await?,
    ))
}

async fn file_info(
    Viewer(caller): Viewer,
    State(state): State<AppState>,
    Path((name, storage, path)): Path<(String, String, String)>,
) -> Result<Json<FileEntry>, ApiError> {
    Ok(Json(
        files::file_info(&state, &caller, &name, &storage, Some(&path)).await?,
    ))
}

async fn file_head(
    Viewer(caller): Viewer,
    State(state): State<AppState>,
    Path((name, storage, path)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    let file = files::file_state(&state, &caller, &name, &storage, &path).await?;
    let flag = |b: bool| HeaderValue::from_static(if b { "true" } else { "false" });
    let mut response = StatusCode::OK.into_response();
    response
        .headers_mut()
        .insert("read-only", flag(file.read_only));
    response
        .headers_mut()
        .insert("currently-printed", flag(file.currently_printed));
    Ok(response)
}
