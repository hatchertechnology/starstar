//! First-admin setup.

use std::time::Duration;

use serde_json::json;
use starstar_shared::Role;

mod common;
use common::*;

fn body(code: &str, user: &str) -> serde_json::Value {
    json!({"setup_code": code, "username": user, "password": "first-admin-pw"})
}
const CODE: &str = "ABCDE-FGHJK-MNPQR-STVWX-YZ012-3";

#[tokio::test]
async fn setup_creates_admin_once_and_discards_code() {
    let (app, state) = test_app();
    state.auth.set_setup_code(Some(CODE.into()));
    let r = post_json(&app, "/api/setup", body(&CODE.to_lowercase(), "kerry")).await;
    assert_eq!(r.status, 201);
    assert_eq!(r.json()["role"], "admin");
    assert!(
        r.headers
            .get_all("set-cookie")
            .iter()
            .any(|v| v.to_str().unwrap().starts_with("starstar_session="))
    );
    assert!(!r.body.contains("first-admin-pw"));
    assert_eq!(
        state.auth.user_by_name("kerry").unwrap().unwrap().role,
        Role::Admin
    );
    assert!(!state.auth.setup_code_matches(CODE));
    assert_eq!(
        post_json(&app, "/api/setup", body(CODE, "eve"))
            .await
            .status,
        409
    );
    // The session it returned works.
    let b = browser_from(&r);
    let req = b
        .cookie_only(axum::http::Request::get("/api/me"))
        .body(axum::body::Body::empty())
        .unwrap();
    assert_eq!(send(&app, req).await.status, 200);
}

#[tokio::test]
async fn setup_rejects_wrong_or_missing_code_and_rate_limits() {
    let (app, state) = test_app();
    // No code set: 403 (and it counts as a failure).
    assert_eq!(
        post_json(&app, "/api/setup", body("anything", "kerry"))
            .await
            .status,
        403
    );
    state.auth.set_setup_code(Some(CODE.into()));
    for _ in 0..4 {
        assert_eq!(
            post_json(&app, "/api/setup", body("WRONG", "kerry"))
                .await
                .status,
            403
        );
    }
    assert_eq!(
        post_json(&app, "/api/setup", body(CODE, "kerry"))
            .await
            .status,
        429
    );
    assert_eq!(state.auth.user_count().unwrap(), 0);
    state.auth.clock().advance(901);
    assert_eq!(
        post_json(&app, "/api/setup", body(CODE, "kerry"))
            .await
            .status,
        201
    );
}

#[tokio::test]
async fn concurrent_setups_create_exactly_one_admin() {
    let (app, state) = test_app();
    state.auth.set_setup_code(Some(CODE.into()));
    let both = async {
        tokio::join!(
            post_json(&app, "/api/setup", body(CODE, "one")),
            post_json(&app, "/api/setup", body(CODE, "two"))
        )
    };
    let (a, b) = tokio::time::timeout(Duration::from_secs(5), both)
        .await
        .expect("setup hung");
    let mut statuses = [a.status.as_u16(), b.status.as_u16()];
    statuses.sort();
    assert_eq!(statuses, [201, 409]);
    assert_eq!(state.auth.user_count().unwrap(), 1);
}

#[tokio::test]
async fn setup_after_users_exist_is_409_even_with_right_code() {
    let (app, state) = test_app();
    state.auth.set_setup_code(Some(CODE.into()));
    create_user(&state, "a", "pw-a-12345", Role::Admin);
    assert_eq!(
        post_json(&app, "/api/setup", body(CODE, "b")).await.status,
        409
    );
}

#[tokio::test]
async fn setup_validates_username_and_password() {
    let (app, state) = test_app();
    state.auth.set_setup_code(Some(CODE.into()));
    assert_eq!(
        post_json(&app, "/api/setup", body(CODE, "Bad Name"))
            .await
            .status,
        400
    );
    let long = "a".repeat(100);
    assert_eq!(
        post_json(&app, "/api/setup", body(CODE, &long))
            .await
            .status,
        400
    );
    let r = post_json(
        &app,
        "/api/setup",
        json!({"setup_code": CODE, "username": "kerry", "password": "short"}),
    )
    .await;
    assert_eq!(r.status, 400);
    assert_eq!(state.auth.user_count().unwrap(), 0);
}
