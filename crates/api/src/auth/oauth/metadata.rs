//! `/.well-known/*` documents (RFC 8414 and RFC 9728).

use axum::extract::State;
use axum::http::HeaderValue;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::AppState;

const SCOPES: [&str; 3] = ["starstar:viewer", "starstar:operator", "starstar:admin"];

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/.well-known/oauth-authorization-server",
            get(authorization_server),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(protected_resource_mcp),
        )
}

fn cached(body: Value) -> Response {
    let mut response = Json(body).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("max-age=300"));
    response
}

async fn authorization_server(State(state): State<AppState>) -> Response {
    let p = &state.config.public_url;
    cached(json!({
        "issuer": p,
        "authorization_endpoint": format!("{p}/oauth/authorize"),
        "token_endpoint": format!("{p}/oauth/token"),
        "registration_endpoint": format!("{p}/oauth/register"),
        "revocation_endpoint": format!("{p}/oauth/revoke"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "revocation_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": SCOPES,
    }))
}

fn resource_document(public_url: &str, resource: &str) -> Response {
    cached(json!({
        "resource": resource,
        "authorization_servers": [public_url],
        "scopes_supported": SCOPES,
        "bearer_methods_supported": ["header"],
    }))
}

async fn protected_resource(State(state): State<AppState>) -> Response {
    let p = &state.config.public_url;
    resource_document(p, p)
}

async fn protected_resource_mcp(State(state): State<AppState>) -> Response {
    let p = &state.config.public_url;
    resource_document(p, &format!("{p}/mcp"))
}
