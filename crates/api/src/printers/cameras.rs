//! Cameras attached to a printer.

use axum::body::Bytes;
use starstar_shared::{Camera, CameraDetail, Role};

use super::paths::validate_camera_id;
use super::{client, find};
use crate::AppState;
use crate::auth::Caller;
use crate::error::ApiError;

pub const LIST_CAMERAS_ROLE: Role = Role::Viewer;
pub const CAMERA_DETAIL_ROLE: Role = Role::Viewer;
pub const SNAPSHOT_ROLE: Role = Role::Viewer;

/// A camera picture. MCP returns it as image content.
pub struct Snapshot {
    pub content_type: String,
    pub bytes: Bytes,
}

pub async fn list_cameras(
    state: &AppState,
    caller: &Caller,
    printer: &str,
) -> Result<Vec<Camera>, ApiError> {
    caller.require(LIST_CAMERAS_ROLE)?;
    let printer = find(state, printer)?;
    let cameras = client(state, &printer).cameras().await?;
    Ok(cameras.into_iter().map(Camera::from).collect())
}

pub async fn camera_detail(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    id: &str,
) -> Result<CameraDetail, ApiError> {
    caller.require(CAMERA_DETAIL_ROLE)?;
    let printer = find(state, printer)?;
    validate_camera_id(id)?;
    Ok(client(state, &printer).camera(id).await?.into())
}

/// A picture from camera `id`, or the default camera when `None`. `Ok(None)`
/// means the printer has no picture yet.
pub async fn snapshot(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    id: Option<&str>,
) -> Result<Option<Snapshot>, ApiError> {
    caller.require(SNAPSHOT_ROLE)?;
    let printer = find(state, printer)?;
    if let Some(id) = id {
        validate_camera_id(id)?;
    }
    Ok(client(state, &printer)
        .snapshot(id)
        .await?
        .map(|(content_type, bytes)| Snapshot {
            content_type,
            bytes,
        }))
}
