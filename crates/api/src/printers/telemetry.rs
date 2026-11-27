//! Read-only printer telemetry: identity, status and storage.

use starstar_shared::{PrinterInfo, PrinterStatus, Role, StorageDevice};

use super::{client, find};
use crate::AppState;
use crate::auth::Caller;
use crate::error::ApiError;

pub const PRINTER_INFO_ROLE: Role = Role::Viewer;
pub const PRINTER_STATUS_ROLE: Role = Role::Viewer;
pub const PRINTER_STORAGE_ROLE: Role = Role::Viewer;

pub async fn printer_info(
    state: &AppState,
    caller: &Caller,
    printer: &str,
) -> Result<PrinterInfo, ApiError> {
    caller.require(PRINTER_INFO_ROLE)?;
    let printer = find(state, printer).await?;
    let client = client(state, &printer);
    let (version, info) = tokio::try_join!(client.version(), client.info())?;

    Ok(PrinterInfo {
        name: printer.name,
        kind: printer.kind,
        firmware: version.firmware,
        api_version: version.api,
        hostname: info.hostname,
        serial: info.serial,
        nozzle_diameter: info.nozzle_diameter,
        mmu: info.mmu,
    })
}

pub async fn printer_status(
    state: &AppState,
    caller: &Caller,
    printer: &str,
) -> Result<PrinterStatus, ApiError> {
    caller.require(PRINTER_STATUS_ROLE)?;
    let printer = find(state, printer).await?;
    let status = client(state, &printer).status().await?.printer;

    Ok(PrinterStatus {
        name: printer.name,
        state: status.state,
        nozzle_temp: status.temp_nozzle,
        nozzle_target: status.target_nozzle,
        bed_temp: status.temp_bed,
        bed_target: status.target_bed,
    })
}

pub async fn printer_storage(
    state: &AppState,
    caller: &Caller,
    printer: &str,
) -> Result<Vec<StorageDevice>, ApiError> {
    caller.require(PRINTER_STORAGE_ROLE)?;
    let printer = find(state, printer).await?;
    let list = client(state, &printer).storage().await?;

    Ok(list
        .storage_list
        .into_iter()
        .map(|s| StorageDevice {
            name: s.name,
            kind: s.kind,
            path: s.path,
            available: s.available,
            read_only: s.read_only,
            free_space: s.free_space,
            total_space: s.total_space,
        })
        .collect())
}
