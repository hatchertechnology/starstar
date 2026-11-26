//! Helpers shared by the API integration tests.
//!
//! **Interface contract:** tests build requests with [`authorize`] so they keep
//! working once real auth lands. The auth part replaces the body of
//! [`authorize`] (create a user with `role` in `state` and attach a real token)
//! without changing its signature.
#![allow(dead_code)]

use axum::http::request::Builder;
use starstar_api::AppState;
use starstar_shared::Role;

/// Adds credentials for a caller with `role` to a request.
///
/// SCAFFOLD: auth is not enforced yet, so this attaches a placeholder token.
pub fn authorize(builder: Builder, _state: &AppState, _role: Role) -> Builder {
    builder.header("authorization", "Bearer scaffold")
}
