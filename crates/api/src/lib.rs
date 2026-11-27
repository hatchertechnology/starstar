//! HTTP API for starstar.
//!
//! The API keeps a registry of printers and exposes one interface over all of
//! them. Each printer's native protocol (PrusaLink) is hidden behind it.

use std::sync::Arc;

use axum::Router;

pub mod auth;
pub mod config;
pub mod error;
pub mod library;
pub mod mcp;
pub mod printers;
pub mod prusalink;
pub mod routes;
pub mod store;
pub mod web;

use config::Config;
use store::PrinterStore;

/// State shared by every request handler.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<PrinterStore>,
    pub library: Arc<library::store::LibraryStore>,
    pub parts: Arc<library::parts::PartsStore>,
    pub http: reqwest::Client,
    pub config: Arc<Config>,
    pub auth: Arc<auth::AuthStore>,
}

/// Builds the API router for the given state.
pub fn router(state: AppState) -> Router {
    // require_auth must stay the outermost auth layer, after every merge (mcp, web).
    routes::router(state.clone())
        .merge(auth::router(state.clone()))
        .merge(mcp::router(state.clone()))
        .merge(web::router(state.clone())) // last: owns the only fallback
        .layer(axum::middleware::from_fn_with_state(
            state,
            auth::require_auth,
        ))
        .layer(axum::middleware::from_fn(auth::security_headers))
}
