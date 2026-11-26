//! Registry, telemetry and job routes.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, put};
use axum::{Json, Router};
use starstar_shared::{PrinterInfo, PrinterStatus, PrinterSummary, RegisterPrinter, StorageDevice};

use crate::AppState;
use crate::auth::{Admin, Operator, Viewer};
use crate::error::ApiError;
use crate::printers::{jobs, registry, telemetry};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/printers", get(list_printers).post(register_printer))
        .route(
            "/api/printers/{name}",
            get(show_printer).delete(remove_printer),
        )
        .route("/api/printers/{name}/info", get(printer_info))
        .route("/api/printers/{name}/status", get(printer_status))
        .route("/api/printers/{name}/storage", get(printer_storage))
        .route("/api/printers/{name}/job", get(current_job))
        .route("/api/printers/{name}/job/{id}", delete(stop_job))
        .route("/api/printers/{name}/job/{id}/pause", put(pause_job))
        .route("/api/printers/{name}/job/{id}/resume", put(resume_job))
        .route("/api/printers/{name}/job/{id}/continue", put(continue_job))
}

async fn list_printers(
    Viewer(caller): Viewer,
    State(state): State<AppState>,
) -> Result<Json<Vec<PrinterSummary>>, ApiError> {
    Ok(Json(registry::list_printers(&state, &caller).await?))
}

async fn register_printer(
    Admin(caller): Admin,
    State(state): State<AppState>,
    Json(req): Json<RegisterPrinter>,
) -> Result<(StatusCode, Json<PrinterSummary>), ApiError> {
    let summary = registry::register_printer(&state, &caller, req).await?;
    Ok((StatusCode::CREATED, Json(summary)))
}

async fn show_printer(
    Viewer(caller): Viewer,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<PrinterSummary>, ApiError> {
    Ok(Json(registry::show_printer(&state, &caller, &name).await?))
}

async fn remove_printer(
    Admin(caller): Admin,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    registry::remove_printer(&state, &caller, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn printer_info(
    Viewer(caller): Viewer,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<PrinterInfo>, ApiError> {
    Ok(Json(telemetry::printer_info(&state, &caller, &name).await?))
}

async fn printer_status(
    Viewer(caller): Viewer,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<PrinterStatus>, ApiError> {
    Ok(Json(
        telemetry::printer_status(&state, &caller, &name).await?,
    ))
}

async fn printer_storage(
    Viewer(caller): Viewer,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<StorageDevice>>, ApiError> {
    Ok(Json(
        telemetry::printer_storage(&state, &caller, &name).await?,
    ))
}

async fn current_job(
    Viewer(caller): Viewer,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Response, ApiError> {
    Ok(match jobs::current_job(&state, &caller, &name).await? {
        Some(job) => Json(job).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    })
}

async fn stop_job(
    Operator(caller): Operator,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, u64)>,
) -> Result<StatusCode, ApiError> {
    jobs::stop_job(&state, &caller, &name, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn pause_job(
    Operator(caller): Operator,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, u64)>,
) -> Result<StatusCode, ApiError> {
    jobs::pause_job(&state, &caller, &name, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn resume_job(
    Operator(caller): Operator,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, u64)>,
) -> Result<StatusCode, ApiError> {
    jobs::resume_job(&state, &caller, &name, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn continue_job(
    Operator(caller): Operator,
    State(state): State<AppState>,
    Path((name, id)): Path<(String, u64)>,
) -> Result<StatusCode, ApiError> {
    jobs::continue_job(&state, &caller, &name, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
