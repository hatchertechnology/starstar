//! File routes. Paths after the storage name are captured whole by `{*path}`.

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use starstar_shared::{DeleteOptions, FileEntry, UploadOptions};

use crate::AppState;
use crate::auth::{Admin, Operator, Viewer};
use crate::error::ApiError;
use crate::printers::files::{self, UploadBody};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/printers/{name}/files/{storage}", get(storage_root))
        .route(
            "/api/printers/{name}/files/{storage}/{*path}",
            // `.head` goes after `.get` so it replaces axum's implicit HEAD-from-GET.
            get(file_info)
                .head(file_head)
                .post(file_action)
                .delete(delete_file)
                .put(upload_file),
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

/// `POST …/{path}/print`. axum cannot route segments after `{*path}`, so the
/// action is the last segment of the captured path.
async fn file_action(
    Operator(caller): Operator,
    State(state): State<AppState>,
    Path((name, storage, path)): Path<(String, String, String)>,
) -> Result<StatusCode, ApiError> {
    let Some(file) = path.strip_suffix("/print") else {
        return Err(ApiError::NotFound("unknown file action".to_owned()));
    };
    files::print_file(&state, &caller, &name, &storage, file).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_file(
    Admin(caller): Admin,
    State(state): State<AppState>,
    Path((name, storage, path)): Path<(String, String, String)>,
    Query(opts): Query<DeleteOptions>,
) -> Result<StatusCode, ApiError> {
    files::delete_file(&state, &caller, &name, &storage, &path, opts).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT`: streams the request body to the printer. The body is never collected
/// or buffered here, so uploads can exceed axum's default body limit.
async fn upload_file(
    Operator(caller): Operator,
    State(state): State<AppState>,
    Path((name, storage, path)): Path<(String, String, String)>,
    Query(opts): Query<UploadOptions>,
    headers: HeaderMap,
    body: Body,
) -> Result<StatusCode, ApiError> {
    let length = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .ok_or_else(|| {
            ApiError::LengthRequired("uploads need a Content-Length header".to_owned())
        })?;
    let upload = UploadBody {
        length,
        body: reqwest::Body::wrap_stream(body.into_data_stream()),
    };
    files::upload_file(&state, &caller, &name, &storage, &path, upload, opts).await?;
    Ok(StatusCode::CREATED)
}
