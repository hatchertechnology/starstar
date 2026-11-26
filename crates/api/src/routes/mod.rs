//! HTTP routes. Handlers are thin: a role extractor first, then one call into
//! [`crate::printers`].

use axum::{Json, Router, routing::get};
use starstar_shared::Health;

use crate::AppState;

mod files;
mod printers;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .merge(printers::router())
        .merge(files::router())
        .with_state(state)
}

async fn health() -> Json<Health> {
    Json(Health::ok())
}
