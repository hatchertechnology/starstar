//! Cameras attached to a printer.

use axum::body::Bytes;
use starstar_shared::{Camera, CameraDetail, CameraSettings, Role};

use super::paths::validate_camera_id;
use super::{client, find};
use crate::AppState;
use crate::auth::Caller;
use crate::error::ApiError;
use crate::prusalink::WireCameraSettings;

pub const LIST_CAMERAS_ROLE: Role = Role::Viewer;
pub const CAMERA_DETAIL_ROLE: Role = Role::Viewer;
pub const SNAPSHOT_ROLE: Role = Role::Viewer;
pub const ORDER_CAMERAS_ROLE: Role = Role::Admin;
pub const REGISTER_CAMERA_ROLE: Role = Role::Admin;
pub const REMOVE_CAMERA_ROLE: Role = Role::Admin;
pub const UPDATE_CAMERA_CONFIG_ROLE: Role = Role::Admin;
pub const RESET_CAMERA_CONFIG_ROLE: Role = Role::Admin;
pub const CONNECT_CAMERA_ROLE: Role = Role::Admin;
pub const DISCONNECT_CAMERA_ROLE: Role = Role::Admin;
pub const TAKE_SNAPSHOT_ROLE: Role = Role::Operator;

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

pub async fn order_cameras(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    ids: Vec<String>,
) -> Result<(), ApiError> {
    caller.require(ORDER_CAMERAS_ROLE)?;
    let printer = find(state, printer)?;
    ids.iter().try_for_each(|id| validate_camera_id(id))?;
    Ok(client(state, &printer).set_camera_order(&ids).await?)
}

pub async fn register_camera(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    id: &str,
    settings: CameraSettings,
) -> Result<(), ApiError> {
    caller.require(REGISTER_CAMERA_ROLE)?;
    let printer = find(state, printer)?;
    validate_camera_id(id)?;
    let settings = WireCameraSettings::from(settings);
    Ok(client(state, &printer)
        .register_camera(id, &settings)
        .await?)
}

pub async fn remove_camera(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    id: &str,
) -> Result<(), ApiError> {
    caller.require(REMOVE_CAMERA_ROLE)?;
    let printer = find(state, printer)?;
    validate_camera_id(id)?;
    Ok(client(state, &printer).remove_camera(id).await?)
}

pub async fn update_camera_config(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    id: &str,
    settings: CameraSettings,
) -> Result<(), ApiError> {
    caller.require(UPDATE_CAMERA_CONFIG_ROLE)?;
    let printer = find(state, printer)?;
    validate_camera_id(id)?;
    let settings = WireCameraSettings::from(settings);
    Ok(client(state, &printer)
        .update_camera_config(id, &settings)
        .await?)
}

pub async fn reset_camera_config(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    id: &str,
) -> Result<(), ApiError> {
    caller.require(RESET_CAMERA_CONFIG_ROLE)?;
    let printer = find(state, printer)?;
    validate_camera_id(id)?;
    Ok(client(state, &printer).reset_camera_config(id).await?)
}

pub async fn connect_camera(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    id: &str,
) -> Result<(), ApiError> {
    caller.require(CONNECT_CAMERA_ROLE)?;
    let printer = find(state, printer)?;
    validate_camera_id(id)?;
    Ok(client(state, &printer).connect_camera(id).await?)
}

pub async fn disconnect_camera(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    id: &str,
) -> Result<(), ApiError> {
    caller.require(DISCONNECT_CAMERA_ROLE)?;
    let printer = find(state, printer)?;
    validate_camera_id(id)?;
    Ok(client(state, &printer).disconnect_camera(id).await?)
}

/// Takes a picture now and returns it.
pub async fn take_snapshot(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    id: &str,
) -> Result<Snapshot, ApiError> {
    caller.require(TAKE_SNAPSHOT_ROLE)?;
    let printer = find(state, printer)?;
    validate_camera_id(id)?;
    let (content_type, bytes) = client(state, &printer).take_snapshot(id).await?;
    Ok(Snapshot {
        content_type,
        bytes,
    })
}
