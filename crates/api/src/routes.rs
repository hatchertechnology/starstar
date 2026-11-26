use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use starstar_shared::{
    Health, PrinterInfo, PrinterStatus, PrinterSummary, RegisterPrinter, StorageDevice,
    is_valid_printer_name,
};

use crate::AppState;
use crate::error::ApiError;
use crate::printers::{self, find};
use crate::store::StoredPrinter;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/printers", get(list_printers).post(register_printer))
        .route(
            "/api/printers/{name}",
            get(show_printer).delete(remove_printer),
        )
        .route("/api/printers/{name}/info", get(printer_info))
        .route("/api/printers/{name}/status", get(printer_status))
        .route("/api/printers/{name}/storage", get(printer_storage))
        .with_state(state)
}

async fn health() -> Json<Health> {
    Json(Health::ok())
}

async fn list_printers(
    State(state): State<AppState>,
) -> Result<Json<Vec<PrinterSummary>>, ApiError> {
    let printers = state.store.list()?;
    Ok(Json(printers.iter().map(StoredPrinter::summary).collect()))
}

async fn register_printer(
    State(state): State<AppState>,
    Json(req): Json<RegisterPrinter>,
) -> Result<(StatusCode, Json<PrinterSummary>), ApiError> {
    validate_registration(&req)?;
    if state.store.get(&req.name)?.is_some() {
        return Err(ApiError::Conflict(format!(
            "printer `{}` already exists",
            req.name
        )));
    }

    let printer = StoredPrinter {
        name: req.name,
        kind: req.kind,
        host: req.host,
        username: req.username,
        password: req.password,
    };

    // Confirm the printer is reachable and the credentials work before saving.
    printers::client(&state, &printer)
        .version()
        .await
        .map_err(|err| ApiError::Upstream(format!("could not verify printer: {err}")))?;

    state.store.insert(printer.clone())?;
    Ok((StatusCode::CREATED, Json(printer.summary())))
}

async fn show_printer(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<PrinterSummary>, ApiError> {
    Ok(Json(find(&state, &name)?.summary()))
}

async fn remove_printer(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    if state.store.remove(&name)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found(&name))
    }
}

async fn printer_info(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<PrinterInfo>, ApiError> {
    let printer = find(&state, &name)?;
    let client = printers::client(&state, &printer);
    let (version, info) = tokio::try_join!(client.version(), client.info())?;

    Ok(Json(PrinterInfo {
        name: printer.name,
        kind: printer.kind,
        firmware: version.firmware,
        api_version: version.api,
        hostname: info.hostname,
        serial: info.serial,
        nozzle_diameter: info.nozzle_diameter,
        mmu: info.mmu,
    }))
}

async fn printer_status(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<PrinterStatus>, ApiError> {
    let printer = find(&state, &name)?;
    let status = printers::client(&state, &printer).status().await?.printer;

    Ok(Json(PrinterStatus {
        name: printer.name,
        state: status.state,
        nozzle_temp: status.temp_nozzle,
        nozzle_target: status.target_nozzle,
        bed_temp: status.temp_bed,
        bed_target: status.target_bed,
    }))
}

async fn printer_storage(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<StorageDevice>>, ApiError> {
    let printer = find(&state, &name)?;
    let list = printers::client(&state, &printer).storage().await?;

    Ok(Json(
        list.storage_list
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
            .collect(),
    ))
}

fn not_found(name: &str) -> ApiError {
    ApiError::NotFound(format!("no printer named `{name}`"))
}

fn validate_registration(req: &RegisterPrinter) -> Result<(), ApiError> {
    if !is_valid_printer_name(&req.name) {
        return Err(ApiError::BadRequest(
            "printer names must be 1-63 lowercase letters, digits or dashes, and cannot start with a dash"
                .to_owned(),
        ));
    }
    if req.host.is_empty() || req.host.contains('/') {
        return Err(ApiError::BadRequest(
            "host must be a hostname or IP address, optionally with a port".to_owned(),
        ));
    }
    if req.username.is_empty() || req.password.is_empty() {
        return Err(ApiError::BadRequest(
            "username and password are required".to_owned(),
        ));
    }
    Ok(())
}
