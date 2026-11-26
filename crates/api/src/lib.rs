//! HTTP API for starstar.
//!
//! The API keeps a registry of printers and exposes one interface over all of
//! them. Each printer's native protocol (PrusaLink) is hidden behind it.

use std::sync::Arc;

use axum::Router;

pub mod auth;
pub mod config;
pub mod error;
pub mod prusalink;
pub mod routes;
pub mod store;

use config::Config;
use store::PrinterStore;

/// State shared by every request handler.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<PrinterStore>,
    pub http: reqwest::Client,
    pub config: Arc<Config>,
    pub auth: Arc<auth::AuthStore>,
}

/// Builds the API router for the given state.
pub fn router(state: AppState) -> Router {
    routes::router(state.clone()).merge(auth::router(state))
}
