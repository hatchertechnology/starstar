//! Files and folders on a printer's storage.

use starstar_shared::{FileEntry, FileState, Role};

use super::paths::{file_segments, resolve_storage};
use super::{client, find};
use crate::AppState;
use crate::auth::Caller;
use crate::error::ApiError;

pub const FILE_INFO_ROLE: Role = Role::Viewer;
pub const FILE_STATE_ROLE: Role = Role::Viewer;

/// Metadata of a file or folder; `path: None` lists the storage root.
pub async fn file_info(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    storage: &str,
    path: Option<&str>,
) -> Result<FileEntry, ApiError> {
    caller.require(FILE_INFO_ROLE)?;
    let printer = find(state, printer)?;
    let segments = path.map(file_segments).transpose()?.unwrap_or_default();
    let client = client(state, &printer);
    let storage = resolve_storage(&client, storage).await?;
    Ok(client.file_info(&storage, &segments).await?.into())
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
    let printer = find(state, printer)?;
    let segments = file_segments(path)?;
    let client = client(state, &printer);
    let storage = resolve_storage(&client, storage).await?;
    let (read_only, currently_printed) = client.file_head(&storage, &segments).await?;
    Ok(FileState {
        read_only,
        currently_printed,
    })
}
