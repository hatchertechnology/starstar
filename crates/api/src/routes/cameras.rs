//! Camera routes.
//!
//! `/cameras/snap` is a static route that wins over `/cameras/{id}`.

use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post, put};
use axum::{Json, Router};
use starstar_shared::{Camera, CameraDetail, CameraSettings};

use crate::AppState;
use crate::auth::{Admin, Operator, Viewer};
use crate::error::ApiError;
use crate::printers::cameras::{self, Snapshot};

pub fn router() -> Router<AppState> {
    // The camera ids `order` and `snap` are shadowed by the static routes
    // below: `PUT …/cameras/order` and `GET …/cameras/snap` win over
    // `{id}`, and the other methods on those two paths answer 405 rather than
    // falling through to `{id}`. Such cameras cannot be registered or removed.
    Router::new()
        .route("/api/printers/{name}/cameras", get(list_cameras))
        .route("/api/printers/{name}/cameras/order", put(order_cameras))
        .route("/api/printers/{name}/cameras/snap", get(default_snapshot))
        .route(
            "/api/printers/{name}/cameras/{id}",
            get(camera_detail)
                .post(register_camera)
                .delete(remove_camera),
        )
        .route(
            "/api/printers/{name}/cameras/{id}/config",
            patch(update_camera_config).delete(reset_camera_config),
        )
        .route(
            "/api/printers/{name}/cameras/{id}/connection",
            post(connect_camera).delete(disconnect_camera),
        )
        .route(
            "/api/printers/{name}/cameras/{id}/snap",
            get(camera_snapshot).post(take_snapshot),
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

async fn order_cameras(
    Admin(caller): Admin,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(ids): Json<Vec<String>>,
) -> Result<StatusCode, ApiError> {
    cameras::order_cameras(&state, &caller, &name, ids).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn register_camera(
    Admin(caller): Admin,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
    Json(settings): Json<CameraSettings>,
) -> Result<StatusCode, ApiError> {
    cameras::register_camera(&state, &caller, &name, &id, settings).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_camera(
    Admin(caller): Admin,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    cameras::remove_camera(&state, &caller, &name, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_camera_config(
    Admin(caller): Admin,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
    Json(settings): Json<CameraSettings>,
) -> Result<StatusCode, ApiError> {
    cameras::update_camera_config(&state, &caller, &name, &id, settings).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reset_camera_config(
    Admin(caller): Admin,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    cameras::reset_camera_config(&state, &caller, &name, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn connect_camera(
    Admin(caller): Admin,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    cameras::connect_camera(&state, &caller, &name, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn disconnect_camera(
    Admin(caller): Admin,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    cameras::disconnect_camera(&state, &caller, &name, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn take_snapshot(
    Operator(caller): Operator,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let snapshot = cameras::take_snapshot(&state, &caller, &name, &id).await?;
    Ok(snapshot_response(Some(snapshot)))
}
