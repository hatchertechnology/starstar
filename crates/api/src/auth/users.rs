//! `/api/me`, `/api/me/password` and `/api/users`.

use axum::routing::get;
use axum::{Json, Router};
use starstar_shared::auth::WhoAmI;

use super::Viewer;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/me", get(me))
}

async fn me(Viewer(caller): Viewer) -> Json<WhoAmI> {
    Json(WhoAmI {
        username: caller.username,
        role: caller.role,
    })
}
