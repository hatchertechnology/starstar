//! Connected apps.

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use starstar_shared::Role;

mod common;
use common::*;

async fn call(app: &Router, token: &str, method: &str, uri: &str) -> TestResponse {
    send(
        app,
        Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

fn grant_id_of(state: &starstar_api::AppState, username: &str) -> Vec<String> {
    let id = state.auth.user_by_name(username).unwrap().unwrap().id;
    state
        .auth
        .list_grants(Some(id))
        .unwrap()
        .into_iter()
        .map(|g| g.id)
        .collect()
}

#[tokio::test]
async fn lists_my_grants_with_client_and_usage() {
    let (app, state) = test_app();
    let v = token_for(&state, "v", Role::Viewer);
    assert_eq!(call(&app, &v, "GET", "/api/me").await.status, 200); // sets last_used_at
    let r = call(&app, &v, "GET", "/api/apps").await;
    assert_eq!(r.status, 200);
    let list = r.json();
    assert_eq!(list.as_array().unwrap().len(), 1);
    let g = &list[0];
    assert_eq!(g["client_id"], "starstar-cli");
    assert_eq!(g["client_name"], "starstar CLI");
    assert_eq!(g["username"], "v");
    assert_eq!(g["role"], "viewer");
    assert_eq!(g["last_used_at"], 1_800_000_000i64);
    assert_eq!(g["expires_at"], 1_800_000_000i64 + 90 * 86400);
}

#[tokio::test]
async fn users_see_only_their_own_and_admins_see_all() {
    let (app, state) = test_app();
    let v = token_for(&state, "v", Role::Viewer);
    token_for(&state, "o", Role::Operator);
    let admin = token_for(&state, "boss", Role::Admin);
    assert_eq!(
        call(&app, &v, "GET", "/api/apps")
            .await
            .json()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call(&app, &v, "GET", "/api/apps?all=true").await.status,
        403
    );
    let all = call(&app, &admin, "GET", "/api/apps?all=true").await;
    assert_eq!(all.json().as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn revoke_my_grant_kills_its_tokens() {
    let (app, state) = test_app();
    let first = token_for(&state, "v", Role::Viewer);
    state.auth.clock().advance(1); // distinct created_at for ordering
    let second = token_for(&state, "v", Role::Viewer);
    let newest = &grant_id_of(&state, "v")[0]; // newest first
    assert_eq!(
        call(&app, &first, "DELETE", &format!("/api/apps/{newest}"))
            .await
            .status,
        204
    );
    assert_eq!(call(&app, &second, "GET", "/api/me").await.status, 401);
    assert_eq!(call(&app, &first, "GET", "/api/me").await.status, 200);
    assert_eq!(
        call(&app, &first, "DELETE", &format!("/api/apps/{newest}"))
            .await
            .status,
        404
    );
}

#[tokio::test]
async fn cannot_revoke_someone_elses_grant_but_admin_can() {
    let (app, state) = test_app();
    let v = token_for(&state, "v", Role::Viewer);
    let o = token_for(&state, "o", Role::Operator);
    let admin = token_for(&state, "boss", Role::Admin);
    let o_grant = &grant_id_of(&state, "o")[0];
    assert_eq!(
        call(&app, &v, "DELETE", &format!("/api/apps/{o_grant}"))
            .await
            .status,
        404
    );
    assert_eq!(call(&app, &o, "GET", "/api/me").await.status, 200);
    assert_eq!(
        call(&app, &admin, "DELETE", &format!("/api/apps/{o_grant}"))
            .await
            .status,
        204
    );
    assert_eq!(call(&app, &o, "GET", "/api/me").await.status, 401);
}

#[tokio::test]
async fn deleting_a_grant_cascades_to_its_token_rows() {
    let (app, state) = test_app();
    let v = token_for(&state, "v", Role::Viewer);
    assert_eq!(state.auth.token_hashes().len(), 2);
    let g = &grant_id_of(&state, "v")[0];
    assert_eq!(
        call(&app, &v, "DELETE", &format!("/api/apps/{g}"))
            .await
            .status,
        204
    );
    assert!(state.auth.token_hashes().is_empty());
}

#[tokio::test]
async fn last_used_is_throttled() {
    let (app, state) = test_app();
    let v = token_for(&state, "v", Role::Viewer);
    let last_used = |state: &starstar_api::AppState| {
        let id = state.auth.user_by_name("v").unwrap().unwrap().id;
        state.auth.list_grants(Some(id)).unwrap()[0].last_used_at
    };
    call(&app, &v, "GET", "/api/me").await;
    assert_eq!(last_used(&state), Some(TEST_START));
    state.auth.clock().advance(30);
    call(&app, &v, "GET", "/api/me").await;
    assert_eq!(last_used(&state), Some(TEST_START));
    state.auth.clock().advance(31);
    call(&app, &v, "GET", "/api/me").await;
    assert_eq!(last_used(&state), Some(TEST_START + 61));
}
