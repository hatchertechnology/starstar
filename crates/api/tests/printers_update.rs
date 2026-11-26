//! Firmware update check and apply.

use axum::body::Body;
use axum::http::{Method, StatusCode};
use serde_json::json;
use starstar_shared::Role;

mod common;
mod fake_prusalink;

use fake_prusalink::{Canned, TestApi};

#[tokio::test]
async fn update_available() {
    let api = TestApi::start().await;
    let r = api
        .get(Role::Admin, "/api/printers/mk4/update/prusalink")
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(
        r.json(),
        json!({"env":"prusalink","available":true,"new_version":"0.8.1"})
    );
}

#[tokio::test]
async fn no_update_is_available_false() {
    let api = TestApi::start().await;
    api.fake.set(
        Method::GET,
        "/api/v1/update/prusalink",
        Canned::empty(204).header("Update-Available", "false"),
    );
    let r = api
        .get(Role::Admin, "/api/printers/mk4/update/prusalink")
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let j = r.json();
    assert_eq!(j["available"], false);
    assert!(j.get("new_version").is_none());
}

#[tokio::test]
async fn applies_update() {
    let api = TestApi::start().await;
    let r = api
        .call(
            Role::Admin,
            Method::POST,
            "/api/printers/mk4/update/prusalink",
            &[],
            Body::empty(),
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.text());
    assert!(
        api.fake
            .authorized(Method::POST)
            .iter()
            .any(|q| q.uri == "/api/v1/update/prusalink")
    );
}

#[tokio::test]
async fn update_conflict_is_409() {
    let api = TestApi::start().await;
    api.fake.set(
        Method::POST,
        "/api/v1/update/prusalink",
        Canned::json(409, json!({"title":"Conflict","text":"print in progress"})),
    );
    let r = api
        .call(
            Role::Admin,
            Method::POST,
            "/api/printers/mk4/update/prusalink",
            &[],
            Body::empty(),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert!(
        r.json()["error"]
            .as_str()
            .unwrap()
            .contains("print in progress")
    );
}

#[tokio::test]
async fn unknown_env_is_400() {
    let api = TestApi::start().await;
    for env in ["bogus", "system"] {
        let r = api
            .get(Role::Admin, &format!("/api/printers/mk4/update/{env}"))
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{env}");
        assert!(r.json()["error"].as_str().unwrap().contains("prusalink"));
    }
    assert!(api.fake.requests().is_empty());
}
