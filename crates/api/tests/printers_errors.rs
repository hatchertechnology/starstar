//! How PrusaLink failures surface through the API.

use std::time::{Duration, Instant};

use axum::http::{Method, StatusCode};
use serde_json::json;
use starstar_api::config::Config;
use starstar_shared::Role;

mod common;
mod fake_prusalink;

use fake_prusalink::{Canned, TestApi};

const STATUS: &str = "/api/v1/status";

#[tokio::test]
async fn upstream_409_carries_printer_text() {
    let api = TestApi::start().await;
    api.fake.set(
        Method::GET,
        STATUS,
        Canned::json(409, json!({"title":"Conflict","text":"Printer is busy"})),
    );
    let r = api.get(Role::Viewer, "/api/printers/mk4/status").await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text());
    assert!(r.json()["error"].as_str().unwrap().contains("busy"));
}

#[tokio::test]
async fn upstream_403_is_403() {
    let api = TestApi::start().await;
    api.fake.set(
        Method::GET,
        STATUS,
        Canned::json(403, json!({"title":"Forbidden","text":"Nope"})),
    );
    let r = api.get(Role::Viewer, "/api/printers/mk4/status").await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert!(r.json()["error"].as_str().unwrap().contains("refused"));
}

#[tokio::test]
async fn upstream_503_is_502() {
    let api = TestApi::start().await;
    api.fake.set(Method::GET, STATUS, Canned::empty(503));
    let r = api.get(Role::Viewer, "/api/printers/mk4/status").await;
    assert_eq!(r.status, StatusCode::BAD_GATEWAY);
    assert!(r.json()["error"].as_str().unwrap().contains("unavailable"));
}

#[tokio::test]
async fn wrong_printer_password_is_502_not_401() {
    let api = TestApi::start().await;
    api.add_printer("bad", &api.fake.addr.to_string(), "wrong");
    let r = api.get(Role::Viewer, "/api/printers/bad/status").await;
    assert_eq!(r.status, StatusCode::BAD_GATEWAY, "{}", r.text());
    let error = r.json()["error"].as_str().unwrap().to_owned();
    assert!(error.contains("stored credentials"), "{error}");
    assert!(!error.contains("wrong"));
}

#[tokio::test]
async fn slow_printer_times_out_with_504() {
    let api = TestApi::start_with(Config {
        printer_timeout: Duration::from_millis(300),
        ..Config::default()
    })
    .await;
    api.fake.set_delay(Duration::from_secs(2));
    let started = Instant::now();
    let r = api.get(Role::Viewer, "/api/printers/mk4/status").await;
    assert_eq!(r.status, StatusCode::GATEWAY_TIMEOUT, "{}", r.text());
    assert!(r.json()["error"].as_str().unwrap().contains("in time"));
    assert!(started.elapsed() < Duration::from_millis(1500));
}

#[tokio::test]
async fn unreachable_printer_is_502() {
    let api = TestApi::start().await;
    api.add_printer("gone", "127.0.0.1:1", "x");
    let started = Instant::now();
    let r = api.get(Role::Viewer, "/api/printers/gone/status").await;
    assert_eq!(r.status, StatusCode::BAD_GATEWAY, "{}", r.text());
    assert!(
        r.json()["error"]
            .as_str()
            .unwrap()
            .contains("could not reach")
    );
    assert!(started.elapsed() < Duration::from_secs(2));
}
