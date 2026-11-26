//! Helpers shared by the API integration tests.
//!
//! **Interface contract:** integration tests build their app with
//! [`test_app`] and add credentials with [`authorize`], so they keep working
//! once real auth lands. The auth part replaces the bodies of [`test_state`]
//! (adding its in-memory store) and [`authorize`] (create a user with a
//! unique username and `role` in `state`, using cheap argon2 parameters, and
//! attach a real bearer token) without changing their signatures.
#![allow(dead_code)]

use std::sync::Arc;

use axum::Router;
use axum::http::request::Builder;
use starstar_api::auth::AuthStore;
use starstar_api::auth::clock::Clock;
use starstar_api::auth::password::Hasher;
use starstar_api::config::Config;
use starstar_api::prusalink::http_client;
use starstar_api::store::{Db, PrinterStore};
use starstar_api::{AppState, router};
use starstar_shared::Role;

/// A fresh in-memory state with default config.
pub const TEST_START: i64 = 1_800_000_000;

pub fn test_state() -> AppState {
    let db = Db::open_in_memory().unwrap();
    AppState {
        store: Arc::new(PrinterStore::from_db(db.clone())),
        auth: Arc::new(
            AuthStore::new(db, Clock::manual(TEST_START), Hasher::fast_for_tests()).unwrap(),
        ),
        http: http_client().unwrap(),
        config: Arc::new(Config::default()),
    }
}

/// A router over a fresh in-memory state, plus that state for [`authorize`].
pub fn test_app() -> (Router, AppState) {
    let state = test_state();
    (router(state.clone()), state)
}

/// Adds credentials for a caller with `role` to a request.
///
/// SCAFFOLD: auth is not enforced yet, so this attaches a placeholder token.
pub fn authorize(builder: Builder, _state: &AppState, _role: Role) -> Builder {
    builder.header("authorization", "Bearer scaffold")
}
