//! Printer operations, one `pub async fn` each, taking `(&AppState, &Caller, printer, …)`.
//!
//! Every operation checks the caller's role first, validates its input, calls
//! the PrusaLink client and translates wire types into `starstar_shared` types.
//! HTTP handlers and MCP tools both call these, so roles cannot drift apart.

use crate::AppState;
use crate::error::ApiError;
use crate::prusalink::PrusaLinkClient;
use crate::store::StoredPrinter;

/// Looks up a registered printer by name.
pub(crate) fn find(state: &AppState, name: &str) -> Result<StoredPrinter, ApiError> {
    state
        .store
        .get(name)?
        .ok_or_else(|| ApiError::NotFound(format!("no printer named `{name}`")))
}

/// A PrusaLink client for `printer`, using the configured timeouts.
pub(crate) fn client(state: &AppState, printer: &StoredPrinter) -> PrusaLinkClient {
    printer
        .client(state.http.clone())
        .with_timeouts(state.config.printer_timeout, state.config.upload_timeout)
}

pub mod jobs;
pub mod registry;
pub mod telemetry;
pub mod transfer;
