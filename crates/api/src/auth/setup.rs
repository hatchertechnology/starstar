//! `POST /api/setup`: create the first admin with the one-time setup code.

use axum::extract::State;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::Response;
use axum::routing::post;
use axum::{Json, Router};
use starstar_shared::auth::{SessionInfo, SetupRequest, is_valid_username};

use super::client_ip::client_ip;
use super::cookies::session_cookies;
use super::password::validate_password;
use super::session::with_cookies;
use crate::AppState;
use crate::error::ApiError;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/setup", post(setup))
}

pub(crate) const USERNAME_RULES: &str = "usernames must be 1-32 lowercase letters, digits, '.', '_' or '-', starting with a letter or digit";

async fn setup(
    State(state): State<AppState>,
    parts: Parts,
    Json(req): Json<SetupRequest>,
) -> Result<Response, ApiError> {
    let auth = &state.auth;
    if auth.user_count().await? > 0 {
        return Err(ApiError::Conflict(
            "setup is closed: an admin already exists".to_owned(),
        ));
    }
    // The key is the IP alone, so a long or odd username cannot grow it.
    let ip = client_ip(&parts, &state.config.trusted_proxies);
    let now = auth.now();
    if auth.setup_limiter().is_blocked(&ip, now) {
        return Err(ApiError::TooManyRequests(
            "too many failed setup attempts, try again later".to_owned(),
        ));
    }
    if !auth.setup_code_matches(&req.setup_code) {
        auth.setup_limiter().record_failure(&ip, now);
        return Err(ApiError::Forbidden("invalid setup code".to_owned()));
    }
    if req.username.len() > 64 || !is_valid_username(&req.username) {
        return Err(ApiError::BadRequest(USERNAME_RULES.to_owned()));
    }
    validate_password(&req.password)?;

    let hash = auth.hash_password(req.password).await?;
    let user = auth.create_first_admin(&req.username, &hash).await?;

    auth.set_setup_code(None);
    auth.setup_limiter().reset(&ip);
    let session = auth.create_session(user.id).await?;
    let info = SessionInfo {
        username: user.username,
        role: user.role,
        csrf_token: session.csrf_token.clone(),
    };
    Ok(with_cookies(
        StatusCode::CREATED,
        session_cookies(&state.config, &session),
        Json(info),
    ))
}
