//! The running print job.

use starstar_shared::{Job, Role};

use super::{client, find};
use crate::AppState;
use crate::auth::Caller;
use crate::error::ApiError;

pub const CURRENT_JOB_ROLE: Role = Role::Viewer;
pub const STOP_JOB_ROLE: Role = Role::Operator;
pub const PAUSE_JOB_ROLE: Role = Role::Operator;
pub const RESUME_JOB_ROLE: Role = Role::Operator;
pub const CONTINUE_JOB_ROLE: Role = Role::Operator;

/// The running job, or `None` when the printer is idle.
pub async fn current_job(
    state: &AppState,
    caller: &Caller,
    printer: &str,
) -> Result<Option<Job>, ApiError> {
    caller.require(CURRENT_JOB_ROLE)?;
    let printer = find(state, printer).await?;
    Ok(client(state, &printer).job().await?.map(Job::from))
}

pub async fn stop_job(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    id: u64,
) -> Result<(), ApiError> {
    caller.require(STOP_JOB_ROLE)?;
    let printer = find(state, printer).await?;
    Ok(client(state, &printer).stop_job(id).await?)
}

pub async fn pause_job(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    id: u64,
) -> Result<(), ApiError> {
    caller.require(PAUSE_JOB_ROLE)?;
    let printer = find(state, printer).await?;
    Ok(client(state, &printer).pause_job(id).await?)
}

pub async fn resume_job(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    id: u64,
) -> Result<(), ApiError> {
    caller.require(RESUME_JOB_ROLE)?;
    let printer = find(state, printer).await?;
    Ok(client(state, &printer).resume_job(id).await?)
}

pub async fn continue_job(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    id: u64,
) -> Result<(), ApiError> {
    caller.require(CONTINUE_JOB_ROLE)?;
    let printer = find(state, printer).await?;
    Ok(client(state, &printer).continue_job(id).await?)
}
