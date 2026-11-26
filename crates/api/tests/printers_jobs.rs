//! Job and transfer endpoints.

use axum::body::Body;
use axum::http::{Method, StatusCode};
use serde_json::json;
use starstar_api::auth::Caller;
use starstar_api::error::ApiError;
use starstar_shared::Role;

mod common;
mod fake_prusalink;

use fake_prusalink::{Canned, TestApi};

#[tokio::test]
async fn shows_current_job() {
    let api = TestApi::start().await;
    let r = api.get(Role::Viewer, "/api/printers/mk4/job").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let j = r.json();
    assert_eq!(j["id"], 42);
    assert_eq!(j["state"], "PRINTING");
    assert_eq!(j["progress"], 37.5);
    assert_eq!(j["time_remaining"], 1800);
    assert_eq!(j["serial_print"], false);
    assert_eq!(j["file"]["display_name"], "cube-12mm.bgcode");
    assert_eq!(j["file"]["modified"], 1700000000);
    assert_eq!(j["file"]["meta"]["filament_used_g"], 3.1);
    assert_eq!(j["file"]["meta"]["layer_height"], 0.2);
}

#[tokio::test]
async fn idle_printer_has_no_job() {
    let api = TestApi::start().await;
    api.fake.set(Method::GET, "/api/v1/job", Canned::empty(204));
    let r = api.get(Role::Viewer, "/api/printers/mk4/job").await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert!(r.body.is_empty());
}

#[tokio::test]
async fn serial_print_job_without_file_decodes() {
    let api = TestApi::start().await;
    api.fake.set(
        Method::GET,
        "/api/v1/job",
        Canned::json(
            200,
            json!({"id":3,"state":"PRINTING","progress":1.0,"time_printing":5,"serial_print":true}),
        ),
    );
    let r = api.get(Role::Viewer, "/api/printers/mk4/job").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let j = r.json();
    assert_eq!(j["serial_print"], true);
    assert!(j["file"].is_null());
}

#[tokio::test]
async fn job_controls_forward_to_printer() {
    let api = TestApi::start().await;
    for (method, suffix, up_path) in [
        (Method::DELETE, "", "/api/v1/job/42"),
        (Method::PUT, "/pause", "/api/v1/job/42/pause"),
        (Method::PUT, "/resume", "/api/v1/job/42/resume"),
        (Method::PUT, "/continue", "/api/v1/job/42/continue"),
    ] {
        let r = api
            .call(
                Role::Operator,
                method.clone(),
                &format!("/api/printers/mk4/job/42{suffix}"),
                &[],
                Body::empty(),
            )
            .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT, "{up_path}: {}", r.text());
        assert!(api.fake.authorized(method).iter().any(|q| q.uri == up_path));
    }
}

#[tokio::test]
async fn pausing_when_busy_is_409() {
    let api = TestApi::start().await;
    api.fake.set(
        Method::PUT,
        "/api/v1/job/42/pause",
        Canned::json(409, json!({"title":"Conflict","text":"Printer is busy"})),
    );
    let r = api
        .call(
            Role::Operator,
            Method::PUT,
            "/api/printers/mk4/job/42/pause",
            &[],
            Body::empty(),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert!(r.json()["error"].as_str().unwrap().contains("busy"));
}

#[tokio::test]
async fn stopping_unknown_job_is_404() {
    let api = TestApi::start().await;
    let r = api
        .call(
            Role::Operator,
            Method::DELETE,
            "/api/printers/mk4/job/99",
            &[],
            Body::empty(),
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert!(r.json()["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn job_on_unknown_printer_is_404() {
    let api = TestApi::start().await;
    let r = api.get(Role::Viewer, "/api/printers/ghost/job").await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert!(r.json()["error"].as_str().unwrap().contains("ghost"));
    assert!(api.fake.requests().is_empty());
}

#[tokio::test]
async fn operations_enforce_their_role_without_http() {
    let api = TestApi::start().await;
    let viewer = Caller {
        username: "v".into(),
        role: Role::Viewer,
    };
    let err = starstar_api::printers::jobs::stop_job(&api.state, &viewer, "mk4", 42)
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::Forbidden(_)));
    assert!(api.fake.authorized(Method::DELETE).is_empty());
}
