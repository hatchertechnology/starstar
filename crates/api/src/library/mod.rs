pub mod blobs;
pub mod parse;
pub mod parts;
pub mod store;

use crate::{AppState, auth::Caller, error::ApiError};
use axum::body::Bytes;
use futures_util::Stream;
use starstar_shared::{LibraryFile, LibraryKind, LibraryQuery, Role};

pub const LIST_LIBRARY_ROLE: Role = Role::Viewer;
pub const UPLOAD_LIBRARY_ROLE: Role = Role::Operator;
pub const DELETE_LIBRARY_ROLE: Role = Role::Admin;

pub async fn list_files(
    state: &AppState,
    caller: &Caller,
    query: LibraryQuery,
) -> Result<Vec<LibraryFile>, ApiError> {
    caller.require(LIST_LIBRARY_ROLE)?;
    Ok(state
        .library
        .list(query.q.as_deref(), query.sort.unwrap_or_default())
        .await?)
}
pub async fn get_file(state: &AppState, caller: &Caller, id: i64) -> Result<LibraryFile, ApiError> {
    caller.require(LIST_LIBRARY_ROLE)?;
    state.library.get(id).await?.ok_or_else(|| missing(id))
}
fn missing(id: i64) -> ApiError {
    ApiError::NotFound(format!("no library file with id {id}"))
}
pub async fn add_file<E>(
    state: &AppState,
    caller: &Caller,
    name: &str,
    body: impl Stream<Item = Result<Bytes, E>> + Unpin,
    len: u64,
) -> Result<(LibraryFile, bool), ApiError> {
    caller.require(UPLOAD_LIBRARY_ROLE)?;
    let kind = LibraryKind::from_name(name).ok_or_else(|| {
        ApiError::BadRequest("only .bgcode and .gcode files can be added to the library".into())
    })?;
    Ok(state
        .library
        .add(
            name,
            kind,
            &caller.username,
            body,
            len,
            state.config.max_upload_bytes,
        )
        .await?)
}
pub async fn preview(state: &AppState, caller: &Caller, id: i64) -> Result<Bytes, ApiError> {
    let file = get_file(state, caller, id).await?;
    let path = state
        .library
        .preview_path(&file)
        .ok_or_else(|| ApiError::NotFound("file has no preview".into()))?;
    match tokio::fs::read(path).await {
        Ok(bytes) => Ok(bytes.into()),
        Err(error) => Err(asset_error(state, id, error).await?),
    }
}
pub async fn open_download(
    state: &AppState,
    caller: &Caller,
    id: i64,
) -> Result<(LibraryFile, tokio::fs::File), ApiError> {
    let file = get_file(state, caller, id).await?;
    let handle = match tokio::fs::File::open(state.library.blob_path(&file)).await {
        Ok(handle) => handle,
        Err(error) => return Err(asset_error(state, id, error).await?),
    };
    Ok((file, handle))
}
async fn asset_error(
    state: &AppState,
    id: i64,
    error: std::io::Error,
) -> Result<ApiError, ApiError> {
    if error.kind() == std::io::ErrorKind::NotFound && state.library.get(id).await?.is_none() {
        return Ok(missing(id));
    }
    Ok(ApiError::Internal("internal error".into()))
}
pub async fn delete_file(state: &AppState, caller: &Caller, id: i64) -> Result<(), ApiError> {
    caller.require(DELETE_LIBRARY_ROLE)?;
    if !state.library.delete(id).await? {
        return Err(missing(id));
    }
    Ok(())
}
