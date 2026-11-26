//! `/api/session`: sign in, who am I, sign out.

use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, SET_COOKIE};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use starstar_shared::auth::{LoginRequest, SESSION_COOKIE, SessionInfo, is_valid_username};

use super::client_ip::client_ip;
use super::cookies::{clear_session_cookies, get_cookie, session_cookies};
use super::crypto::hash_token;
use super::session_from_parts;
use super::store::sessions::SessionCaller;
use crate::AppState;
use crate::error::ApiError;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/session", get(show).post(login).delete(logout))
}

/// Rate-limit key: SHA-256 of the lowercased username, plus the client IP.
/// Hashing bounds the key length whatever an attacker sends.
pub(crate) fn limiter_key(username: &str, ip: &str) -> String {
    let digest: String = hash_token(&username.to_lowercase())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{digest}|{ip}")
}

fn bad_login() -> ApiError {
    ApiError::Unauthorized("invalid username or password".to_owned())
}

/// A response with the session cookies set and caching disabled.
pub(crate) fn with_cookies(
    status: StatusCode,
    cookies: [HeaderValue; 2],
    body: impl IntoResponse,
) -> Response {
    let mut response = (status, body).into_response();
    let headers = response.headers_mut();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    for cookie in cookies {
        headers.append(SET_COOKIE, cookie);
    }
    response
}

async fn login(
    State(state): State<AppState>,
    parts: Parts,
    Json(req): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    // Malformed usernames can never exist: fail before touching the limiter
    // or the store, so junk keys cannot grow the limiter.
    if req.username.len() > 64 || !is_valid_username(&req.username.to_lowercase()) {
        return Err(bad_login());
    }
    let key = limiter_key(
        &req.username,
        &client_ip(&parts, &state.config.trusted_proxies),
    );
    let now = state.auth.now();
    let limiter = state.auth.login_limiter();
    // Reserve the attempt before any await, so parallel guesses all count.
    if !limiter.try_begin(&key, now) {
        return Err(ApiError::TooManyRequests(
            "too many failed sign-in attempts, try again later".to_owned(),
        ));
    }

    let hasher = state.auth.hasher();
    let user = state.auth.user_by_name(&req.username.to_lowercase())?;
    let password = req.password;
    let ok = match &user {
        Some(user) => {
            let hash = user.password_hash.clone();
            let verified = tokio::task::spawn_blocking(move || hasher.verify(&password, &hash))
                .await
                .map_err(|_| ApiError::Internal("internal error".to_owned()))?;
            verified && !user.disabled
        }
        None => {
            tokio::task::spawn_blocking(move || hasher.dummy_verify(&password))
                .await
                .map_err(|_| ApiError::Internal("internal error".to_owned()))?;
            false
        }
    };
    let Some(user) = user.filter(|_| ok) else {
        return Err(bad_login());
    };

    limiter.reset(&key);
    let session = state.auth.create_session(user.id)?;
    let info = SessionInfo {
        username: user.username,
        role: user.role,
        csrf_token: session.csrf_token.clone(),
    };
    Ok(with_cookies(
        StatusCode::OK,
        session_cookies(&state.config, &session),
        Json(info),
    ))
}

async fn show(State(state): State<AppState>, parts: Parts) -> Result<Json<SessionInfo>, ApiError> {
    let session = session_from_parts(&parts, &state)?;
    Ok(Json(SessionInfo {
        username: session.caller.username,
        role: session.caller.role,
        csrf_token: session.csrf_token,
    }))
}

async fn logout(State(state): State<AppState>, parts: Parts) -> Result<Response, ApiError> {
    // `require_auth` has already checked the cookie and the CSRF token.
    if parts.extensions.get::<SessionCaller>().is_none() {
        return Err(ApiError::BadRequest("no browser session".to_owned()));
    }
    if let Some(token) = get_cookie(&parts.headers, SESSION_COOKIE) {
        state.auth.delete_session(&token)?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    for cookie in clear_session_cookies(&state.config) {
        response.headers_mut().append(SET_COOKIE, cookie);
    }
    Ok(response)
}
