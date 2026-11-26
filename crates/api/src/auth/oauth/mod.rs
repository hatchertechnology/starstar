//! The OAuth 2.1 authorization server: metadata, registration, authorize,
//! token and revoke.
//!
//! Note on `resource` (RFC 8707): the audience is deliberately shared by
//! `/api` and `/mcp`. A token issued for either works on both, bounded by the
//! grant's role.

use axum::Json;
use axum::Router;
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, HeaderValue};
use axum::response::{IntoResponse, Response};
use starstar_shared::auth::OAuthErrorBody;

use super::store::AuthStoreError;
use crate::AppState;

pub mod authorize;
pub mod metadata;
pub mod redirect;
pub mod register;

/// An RFC 6749 error response (JSON body, `no-store`).
#[derive(Debug)]
pub struct OAuthError {
    pub status: StatusCode,
    pub error: &'static str,
    pub description: String,
}

impl OAuthError {
    fn new(status: StatusCode, error: &'static str, description: impl Into<String>) -> Self {
        Self {
            status,
            error,
            description: description.into(),
        }
    }

    pub fn invalid_request(description: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", description)
    }

    pub fn invalid_client(description: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "invalid_client", description)
    }

    pub fn invalid_grant(description: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_grant", description)
    }

    pub fn unsupported_grant_type(description: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            description,
        )
    }

    pub fn invalid_client_metadata(description: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid_client_metadata",
            description,
        )
    }

    pub fn invalid_redirect_uri(description: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_redirect_uri", description)
    }

    pub fn too_many_requests() -> Self {
        Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            "invalid_request",
            "too many failed attempts, try again later",
        )
    }

    pub fn temporarily_unavailable() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "temporarily_unavailable",
            "too many authorizations are in progress, try again shortly",
        )
    }

    pub fn server_error() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "internal error",
        )
    }
}

impl From<AuthStoreError> for OAuthError {
    fn from(_: AuthStoreError) -> Self {
        Self::server_error()
    }
}

impl IntoResponse for OAuthError {
    fn into_response(self) -> Response {
        let body = OAuthErrorBody {
            error: self.error.to_owned(),
            error_description: Some(self.description),
        };
        let mut response = (self.status, Json(body)).into_response();
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(authorize::routes())
        .merge(metadata::routes())
        .merge(register::routes())
}
