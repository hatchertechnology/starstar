//! PrusaLink update check and apply.

use starstar_shared::{Role, UpdateInfo};

use super::paths::validate_update_env;
use super::{client, find};
use crate::AppState;
use crate::auth::Caller;
use crate::error::ApiError;

pub const CHECK_UPDATE_ROLE: Role = Role::Admin;
pub const APPLY_UPDATE_ROLE: Role = Role::Admin;

pub async fn check_update(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    env: &str,
) -> Result<UpdateInfo, ApiError> {
    caller.require(CHECK_UPDATE_ROLE)?;
    validate_update_env(env)?;
    let printer = find(state, printer).await?;
    let (available, new_version) = client(state, &printer).check_update(env).await?;
    Ok(UpdateInfo {
        env: env.to_owned(),
        available,
        new_version,
    })
}

pub async fn apply_update(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    env: &str,
) -> Result<(), ApiError> {
    caller.require(APPLY_UPDATE_ROLE)?;
    validate_update_env(env)?;
    let printer = find(state, printer).await?;
    Ok(client(state, &printer).apply_update(env).await?)
}
