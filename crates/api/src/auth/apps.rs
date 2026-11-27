//! `/api/apps`: connected apps (OAuth grants).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{Json, Router};
use serde::Deserialize;
use starstar_shared::Role;
use starstar_shared::auth::AppGrant;

use super::Viewer;
use crate::AppState;
use crate::error::ApiError;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/apps", get(list))
        .route("/api/apps/{id}", delete(revoke))
}

#[derive(Deserialize)]
struct ListQuery {
    all: Option<bool>,
}

async fn user_id(state: &AppState, username: &str) -> Result<i64, ApiError> {
    state
        .auth
        .user_by_name(username)
        .await?
        .map(|u| u.id)
        .ok_or_else(|| ApiError::Unauthorized("sign in required".to_owned()))
}

async fn list(
    State(state): State<AppState>,
    Viewer(caller): Viewer,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<AppGrant>>, ApiError> {
    let owner = if q.all.unwrap_or(false) {
        caller.require(Role::Admin)?;
        None
    } else {
        Some(user_id(&state, &caller.username).await?)
    };
    Ok(Json(state.auth.list_grants(owner).await?))
}

async fn revoke(
    State(state): State<AppState>,
    Viewer(caller): Viewer,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let owner = if caller.role >= Role::Admin {
        None
    } else {
        Some(user_id(&state, &caller.username).await?)
    };
    if state.auth.revoke_grant(&id, owner).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound("no app grant with that id".to_owned()))
    }
}
