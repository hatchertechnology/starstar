//! The file transfer to the printer that is in progress.

use starstar_shared::{Role, Transfer};

use super::{client, find};
use crate::AppState;
use crate::auth::Caller;
use crate::error::ApiError;

pub const CURRENT_TRANSFER_ROLE: Role = Role::Viewer;
pub const CANCEL_TRANSFER_ROLE: Role = Role::Operator;

/// The running transfer, or `None`. PrusaLink often omits the transfer's id,
/// so it is then taken from the status endpoint (the only extra call).
pub async fn current_transfer(
    state: &AppState,
    caller: &Caller,
    printer: &str,
) -> Result<Option<Transfer>, ApiError> {
    caller.require(CURRENT_TRANSFER_ROLE)?;
    let printer = find(state, printer)?;
    let client = client(state, &printer);
    let Some(wire) = client.transfer().await? else {
        return Ok(None);
    };
    let mut transfer = Transfer::from(wire);
    if transfer.id.is_none() {
        transfer.id = client.status().await?.transfer.and_then(|t| t.id);
    }
    Ok(Some(transfer))
}

pub async fn cancel_transfer(
    state: &AppState,
    caller: &Caller,
    printer: &str,
    id: u64,
) -> Result<(), ApiError> {
    caller.require(CANCEL_TRANSFER_ROLE)?;
    let printer = find(state, printer)?;
    Ok(client(state, &printer).cancel_transfer(id).await?)
}
