//! End-to-end tests of the API against a fake PrusaLink printer.
//!
//! The fake answers like the real printer: a Digest challenge first, then JSON
//! once the request carries a Digest Authorization header.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use diqwest::WithDigestAuth;
use serde_json::{Value, json};
use starstar_api::AppState;
use starstar_shared::Role;
use tower::ServiceExt;

mod common;
mod fake_prusalink;

use fake_prusalink::{FakePrinter, PASSWORD, USERNAME};

/// Sends one request as `role` and returns the status and raw body text.
async fn send(
    app: &Router,
    state: &AppState,
    role: Role,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, String) {
    let mut builder = common::authorize(Request::builder().method(method).uri(uri), state, role);
    let body = match body {
        Some(json) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(json.to_string())
        }
        None => Body::empty(),
    };

    let response = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

fn registration(name: &str, host: &str) -> Value {
    json!({
        "name": name,
        "kind": "mk4",
        "host": host,
        "username": USERNAME,
        "password": PASSWORD,
    })
}

#[tokio::test]
async fn registers_printer_and_proxies_read_only_calls() {
    let printer = FakePrinter::start().await.addr;
    let (app, state) = common::test_app();

    let (status, body) = send(
        &app,
        &state,
        Role::Admin,
        Method::POST,
        "/api/printers",
        Some(registration("mk4", &printer.to_string())),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert!(!body.contains(PASSWORD), "password leaked in response");

    let (status, body) = send(
        &app,
        &state,
        Role::Viewer,
        Method::GET,
        "/api/printers",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let list: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert!(!body.contains(PASSWORD), "password leaked in listing");

    let (status, body) = send(
        &app,
        &state,
        Role::Viewer,
        Method::GET,
        "/api/printers/mk4/info",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let info: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(info["kind"], "mk4");
    assert_eq!(info["firmware"], "8.0.2+17092");
    assert_eq!(info["api_version"], "2.0.0");
    assert_eq!(info["hostname"], "prusa-mk4");

    let (status, body) = send(
        &app,
        &state,
        Role::Viewer,
        Method::GET,
        "/api/printers/mk4/status",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let status_json: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(status_json["state"], "PRINTING");
    assert_eq!(status_json["nozzle_temp"], 215.0);
    assert_eq!(status_json["bed_target"], 60.0);

    let (status, body) = send(
        &app,
        &state,
        Role::Viewer,
        Method::GET,
        "/api/printers/mk4/storage",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let storage: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(storage[0]["path"], "/usb/");
    assert_eq!(storage[0]["kind"], "USB");
    assert_eq!(storage[0]["available"], true);

    let (status, _) = send(
        &app,
        &state,
        Role::Admin,
        Method::DELETE,
        "/api/printers/mk4",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send(
        &app,
        &state,
        Role::Viewer,
        Method::GET,
        "/api/printers/mk4",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rejects_invalid_requests() {
    let (app, state) = common::test_app();

    let (status, _) = send(
        &app,
        &state,
        Role::Admin,
        Method::POST,
        "/api/printers",
        Some(registration("MK4", "127.0.0.1:1")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send(
        &app,
        &state,
        Role::Admin,
        Method::POST,
        "/api/printers",
        Some(registration("mk4", "http://127.0.0.1:1")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send(
        &app,
        &state,
        Role::Viewer,
        Method::GET,
        "/api/printers/ghost/info",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = send(
        &app,
        &state,
        Role::Admin,
        Method::DELETE,
        "/api/printers/ghost",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unreachable_printer_is_not_registered() {
    let (app, state) = common::test_app();

    let (status, body) = send(
        &app,
        &state,
        Role::Admin,
        Method::POST,
        "/api/printers",
        Some(registration("mk4", "127.0.0.1:1")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");

    let (status, _) = send(
        &app,
        &state,
        Role::Viewer,
        Method::GET,
        "/api/printers/mk4",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn duplicate_names_are_rejected() {
    let printer = FakePrinter::start().await.addr;
    let (app, state) = common::test_app();

    let (status, _) = send(
        &app,
        &state,
        Role::Admin,
        Method::POST,
        "/api/printers",
        Some(registration("mk4", &printer.to_string())),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = send(
        &app,
        &state,
        Role::Admin,
        Method::POST,
        "/api/printers",
        Some(registration("mk4", &printer.to_string())),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

fn signed(method: &str, uri: &str) -> String {
    let mut prompt = digest_auth::parse(
        r#"Digest realm="PrusaLink", nonce="abc123", qop="auth", algorithm=MD5"#,
    )
    .unwrap();
    let ctx = digest_auth::AuthContext::new_with_method(
        USERNAME,
        PASSWORD,
        uri,
        None::<&[u8]>,
        digest_auth::HttpMethod::from(method.to_owned()),
    );
    prompt.respond(&ctx).unwrap().to_header_string()
}

#[tokio::test]
async fn fake_rejects_wrong_digest_password() {
    let fake = FakePrinter::start().await;
    let url = format!("http://{}/api/version", fake.addr);
    let client = reqwest::Client::new();

    let ok = client
        .get(&url)
        .send_digest_auth((USERNAME, PASSWORD))
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);

    let bad = client
        .get(&url)
        .send_digest_auth((USERNAME, "wrong"))
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn fake_rejects_digest_for_another_uri_or_method() {
    let fake = FakePrinter::start().await;
    let client = reqwest::Client::new();
    let base = format!("http://{}", fake.addr);
    let auth = signed("GET", "/api/version");

    let right = client
        .get(format!("{base}/api/version"))
        .header(header::AUTHORIZATION, &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(right.status(), StatusCode::OK);

    let other_uri = client
        .get(format!("{base}/api/v1/info"))
        .header(header::AUTHORIZATION, &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(other_uri.status(), StatusCode::UNAUTHORIZED);

    let other_method = client
        .head(format!("{base}/api/version"))
        .header(header::AUTHORIZATION, &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(other_method.status(), StatusCode::UNAUTHORIZED);
}
