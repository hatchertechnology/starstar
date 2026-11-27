//! HTTP routes. Handlers are thin: a role extractor first, then one call into
//! [`crate::printers`].

use std::time::Duration;

use axum::{Json, Router, extract::State, routing::get};
use starstar_shared::Health;

use crate::AppState;
use crate::error::ApiError;

mod cameras;
mod files;
mod parts;
mod printers;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .merge(printers::router())
        .merge(files::router())
        .merge(parts::router())
        .merge(cameras::router())
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Result<Json<Health>, ApiError> {
    match tokio::time::timeout(Duration::from_secs(2), state.store.ping()).await {
        Ok(Ok(())) => Ok(Json(Health::ok())),
        Ok(Err(_)) | Err(_) => Err(ApiError::ServiceUnavailable(
            "database unavailable".to_owned(),
        )),
    }
}
