//! Camera routes.
//!
//! `/cameras/snap` is a static route that wins over `/cameras/{id}`.

use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use starstar_shared::{Camera, CameraDetail};

use crate::AppState;
use crate::auth::Viewer;
use crate::error::ApiError;
use crate::printers::cameras::{self, Snapshot};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/printers/{name}/cameras", get(list_cameras))
        .route("/api/printers/{name}/cameras/snap", get(default_snapshot))
        .route("/api/printers/{name}/cameras/{id}", get(camera_detail))
        .route(
            "/api/printers/{name}/cameras/{id}/snap",
            get(camera_snapshot),
        )
}

/// 200 with the picture and `Cache-Control: no-store`, or 204.
pub(super) fn snapshot_response(snapshot: Option<Snapshot>) -> Response {
    match snapshot {
        Some(Snapshot {
            content_type,
            bytes,
        }) => {
            let mut response = bytes.into_response();
            if let Ok(value) = HeaderValue::from_str(&content_type) {
                response.headers_mut().insert(header::CONTENT_TYPE, value);
            }
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

async fn list_cameras(
    Viewer(caller): Viewer,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<Camera>>, ApiError> {
    Ok(Json(cameras::list_cameras(&state, &caller, &name).await?))
}

async fn camera_detail(
    Viewer(caller): Viewer,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
) -> Result<Json<CameraDetail>, ApiError> {
    Ok(Json(
        cameras::camera_detail(&state, &caller, &name, &id).await?,
    ))
}

async fn default_snapshot(
    Viewer(caller): Viewer,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Response, ApiError> {
    Ok(snapshot_response(
        cameras::snapshot(&state, &caller, &name, None).await?,
    ))
}

async fn camera_snapshot(
    Viewer(caller): Viewer,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    Ok(snapshot_response(
        cameras::snapshot(&state, &caller, &name, Some(&id)).await?,
    ))
}
