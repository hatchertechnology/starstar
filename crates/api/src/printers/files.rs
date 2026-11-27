//! Files and folders on a printer's storage.

use starstar_shared::{DeleteOptions, FileEntry, FileState, Role, UploadOptions};

use super::paths::{file_segments, resolve_storage};
use super::{client, find};
use crate::AppState;
use crate::auth::Caller;
use crate::error::ApiError;

pub const FILE_INFO_ROLE: Role = Role::Viewer;
pub const FILE_STATE_ROLE: Role = Role::Viewer;
pub const PRINT_FILE_ROLE: Role = Role::Operator;
pub const DELETE_FILE_ROLE: Role = Role::Admin;
pub const UPLOAD_FILE_ROLE: Role = Role::Operator;

/// Metadata of a file or folder; `path: None` lists the storage root.
pub async fn file_info(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    storage: &str,
    path: Option<&str>,
) -> Result<FileEntry, ApiError> {
    caller.require(FILE_INFO_ROLE)?;
    let printer = find(state, printer).await?;
    let segments = path.map(file_segments).transpose()?.unwrap_or_default();
    let client = client(state, &printer);
    let storage = resolve_storage(&client, storage).await?;
    Ok(client.file_info(&storage, &segments).await?.into())
}

/// The file's preview image from the printer: (content type, bytes).
/// 404 when the printer reports no thumbnail, or a link that is not a path
/// on the same printer.
pub async fn file_thumbnail(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    storage: &str,
    path: &str,
) -> Result<(String, axum::body::Bytes), ApiError> {
    caller.require(FILE_INFO_ROLE)?;
    let printer = find(state, printer).await?;
    let segments = file_segments(path)?;
    let client = client(state, &printer);
    let storage = resolve_storage(&client, storage).await?;
    let file = client.file_info(&storage, &segments).await?;
    let no_preview = || ApiError::NotFound("file has no preview".to_owned());
    let link = file.thumbnail_ref().ok_or_else(no_preview)?;
    match client.thumbnail(link).await {
        Ok(image) => Ok(image),
        Err(crate::prusalink::PrusaError::Decode(_)) => Err(no_preview()),
        Err(e) => Err(e.into()),
    }
}

/// What a `HEAD` on the file says: read-only and currently being printed.
pub async fn file_state(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    storage: &str,
    path: &str,
) -> Result<FileState, ApiError> {
    caller.require(FILE_STATE_ROLE)?;
    let printer = find(state, printer).await?;
    let segments = file_segments(path)?;
    let client = client(state, &printer);
    let storage = resolve_storage(&client, storage).await?;
    let (read_only, currently_printed) = client.file_head(&storage, &segments).await?;
    Ok(FileState {
        read_only,
        currently_printed,
    })
}

/// Starts printing a file that is already on the printer.
pub async fn print_file(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    storage: &str,
    path: &str,
) -> Result<(), ApiError> {
    caller.require(PRINT_FILE_ROLE)?;
    let printer = find(state, printer).await?;
    let segments = file_segments(path)?;
    let client = client(state, &printer);
    let storage = resolve_storage(&client, storage).await?;
    Ok(client.print_file(&storage, &segments).await?)
}

pub async fn delete_file(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    storage: &str,
    path: &str,
    opts: DeleteOptions,
) -> Result<(), ApiError> {
    caller.require(DELETE_FILE_ROLE)?;
    let printer = find(state, printer).await?;
    let segments = file_segments(path)?;
    let client = client(state, &printer);
    let storage = resolve_storage(&client, storage).await?;
    Ok(client.delete_file(&storage, &segments, opts.force).await?)
}

/// A file being uploaded: its exact length and a body that yields that many bytes.
///
/// HTTP handlers hand over the request body as a stream, so large files are
/// never held in memory.
pub struct UploadBody {
    pub length: u64,
    pub body: reqwest::Body,
}

impl UploadBody {
    /// For callers that already hold the whole file (MCP enforces its own,
    /// smaller `mcp_max_upload_bytes` first).
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            length: bytes.len() as u64,
            body: reqwest::Body::from(bytes),
        }
    }
}

pub(crate) fn too_large_message(max: u64) -> String {
    if max < 1024 * 1024 {
        format!("upload is larger than the {max} bytes limit")
    } else {
        format!("upload is larger than the {} MB limit", max / (1024 * 1024))
    }
}

pub async fn upload_file(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    storage: &str,
    path: &str,
    upload: UploadBody,
    opts: UploadOptions,
) -> Result<(), ApiError> {
    caller.require(UPLOAD_FILE_ROLE)?;
    let max = state.config.max_upload_bytes;
    if upload.length > max {
        return Err(ApiError::PayloadTooLarge(too_large_message(max)));
    }
    let printer = find(state, printer).await?;
    let segments = file_segments(path)?;
    let client = client(state, &printer);
    let storage = resolve_storage(&client, storage).await?;
    Ok(client
        .upload(
            &storage,
            &segments,
            upload.length,
            upload.body,
            opts.overwrite,
            opts.print,
        )
        .await?)
}
