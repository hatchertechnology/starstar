//! `/api/me`, `/api/me/password` and `/api/users`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::routing::{get, patch, put};
use axum::{Json, Router};
use starstar_shared::auth::{
    ChangePassword, NewUser, SESSION_COOKIE, UpdateUser, UserSummary, WhoAmI, is_valid_username,
};

use super::client_ip::client_ip;
use super::cookies::get_cookie;
use super::password::{MAX_PASSWORD_CHARS, validate_password};
use super::session::limiter_key;
use super::setup::USERNAME_RULES;
use super::{Admin, Viewer};
use crate::AppState;
use crate::error::ApiError;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/me", get(me))
        .route("/api/me/password", put(change_password))
        .route("/api/users", get(list_users).post(create_user))
        .route(
            "/api/users/{username}",
            patch(update_user).delete(remove_user),
        )
}

async fn me(Viewer(caller): Viewer) -> Json<WhoAmI> {
    Json(WhoAmI {
        username: caller.username,
        role: caller.role,
    })
}

async fn list_users(
    State(state): State<AppState>,
    _admin: Admin,
) -> Result<Json<Vec<UserSummary>>, ApiError> {
    let users = state.auth.list_users()?;
    Ok(Json(users.iter().map(|u| u.summary()).collect()))
}

async fn create_user(
    State(state): State<AppState>,
    _admin: Admin,
    Json(req): Json<NewUser>,
) -> Result<(StatusCode, Json<UserSummary>), ApiError> {
    if req.username.len() > 64 || !is_valid_username(&req.username) {
        return Err(ApiError::BadRequest(USERNAME_RULES.to_owned()));
    }
    validate_password(&req.password)?;
    let hash = state.auth.hash_password(req.password).await?;
    let user = state.auth.create_user(&req.username, &hash, req.role)?;
    Ok((StatusCode::CREATED, Json(user.summary())))
}

async fn update_user(
    State(state): State<AppState>,
    _admin: Admin,
    Path(username): Path<String>,
    Json(req): Json<UpdateUser>,
) -> Result<Json<UserSummary>, ApiError> {
    if req.role.is_none() && req.disabled.is_none() {
        return Err(ApiError::BadRequest("nothing to change".to_owned()));
    }
    let user = state.auth.update_user(&username, req.role, req.disabled)?;
    Ok(Json(user.summary()))
}

async fn remove_user(
    State(state): State<AppState>,
    _admin: Admin,
    Path(username): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.auth.remove_user(&username)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn change_password(
    State(state): State<AppState>,
    Viewer(caller): Viewer,
    parts: Parts,
    Json(req): Json<ChangePassword>,
) -> Result<StatusCode, ApiError> {
    let auth = &state.auth;
    let user = auth
        .user_by_name(&caller.username)?
        .ok_or_else(|| ApiError::Unauthorized("sign in required".to_owned()))?;

    // Guessing the current password goes through the sign-in limiter.
    let key = limiter_key(
        &caller.username,
        &client_ip(&parts, &state.config.trusted_proxies),
    );
    let now = auth.now();
    // Reserve the attempt before any await, so parallel guesses all count.
    if !auth.login_limiter().try_begin(&key, now) {
        return Err(ApiError::TooManyRequests(
            "too many failed attempts, try again later".to_owned(),
        ));
    }
    let ok = req.current_password.chars().count() <= MAX_PASSWORD_CHARS
        && auth
            .verify_password(req.current_password, user.password_hash.clone())
            .await?;
    if !ok {
        return Err(ApiError::Forbidden(
            "current password is incorrect".to_owned(),
        ));
    }
    auth.login_limiter().reset(&key);

    validate_password(&req.new_password)?;
    let new_hash = auth.hash_password(req.new_password).await?;
    auth.set_password(user.id, &new_hash)?;
    // Everything else signed in as this user must sign in again, and every
    // connected app is revoked.
    let keep = get_cookie(&parts.headers, SESSION_COOKIE);
    auth.delete_other_sessions(user.id, keep.as_deref())?;
    auth.revoke_user_grants(user.id)?;
    Ok(StatusCode::NO_CONTENT)
}
