//! End-to-end tests of the API against a fake PrusaLink printer.
//!
//! The fake answers like the real printer: a Digest challenge first, then JSON
//! once the request carries a Digest Authorization header.

use std::net::SocketAddr;

use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::{Value, json};
use tower::ServiceExt;

mod common;

const USERNAME: &str = "maker";
const PASSWORD: &str = "hunter2";

fn guarded(headers: &HeaderMap, body: Value) -> Response {
    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("Digest "));

    if authorized {
        Json(body).into_response()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                r#"Digest realm="PrusaLink", nonce="abc123", qop="auth", algorithm=MD5"#,
            )],
        )
            .into_response()
    }
}

async fn fake_printer() -> SocketAddr {
    let app = Router::new()
        .route(
            "/api/version",
            get(|headers: HeaderMap| async move {
                guarded(
                    &headers,
                    json!({ "api": "2.0.0", "server": "2.1.2", "firmware": "8.0.2+17092", "printer": "1.4.0" }),
                )
            }),
        )
        .route(
            "/api/v1/info",
            get(|headers: HeaderMap| async move {
                guarded(
                    &headers,
                    json!({ "hostname": "prusa-mk4", "serial": "10589-3742441631719910", "nozzle_diameter": 0.4, "mmu": false }),
                )
            }),
        )
        .route(
            "/api/v1/status",
            get(|headers: HeaderMap| async move {
                guarded(
                    &headers,
                    json!({ "printer": { "state": "IDLE", "temp_nozzle": 24.6, "target_nozzle": 0.0, "temp_bed": 23.3, "target_bed": 0.0 } }),
                )
            }),
        )
        .route(
            "/api/v1/storage",
            get(|headers: HeaderMap| async move {
                guarded(
                    &headers,
                    json!({ "storage_list": [{ "path": "/usb/", "name": "usb", "type": "USB", "read_only": false, "available": true }] }),
                )
            }),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

/// Sends one request and returns the status and raw body text.
async fn send(
    app: &Router,
    state: &starstar_api::AppState,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, String) {
    let mut builder = common::authorize(
        Request::builder().method(method).uri(uri),
        state,
        starstar_shared::Role::Admin,
    );
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
    let printer = fake_printer().await;
    let (app, state) = common::test_app();

    let (status, body) = send(
        &app,
        &state,
        Method::POST,
        "/api/printers",
        Some(registration("mk4", &printer.to_string())),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert!(!body.contains(PASSWORD), "password leaked in response");

    let (status, body) = send(&app, &state, Method::GET, "/api/printers", None).await;
    assert_eq!(status, StatusCode::OK);
    let list: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert!(!body.contains(PASSWORD), "password leaked in listing");

    let (status, body) = send(&app, &state, Method::GET, "/api/printers/mk4/info", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let info: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(info["kind"], "mk4");
    assert_eq!(info["firmware"], "8.0.2+17092");
    assert_eq!(info["api_version"], "2.0.0");
    assert_eq!(info["hostname"], "prusa-mk4");

    let (status, body) = send(&app, &state, Method::GET, "/api/printers/mk4/status", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let status_json: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(status_json["state"], "IDLE");
    assert_eq!(status_json["nozzle_temp"], 24.6);
    assert_eq!(status_json["bed_target"], 0.0);

    let (status, body) = send(&app, &state, Method::GET, "/api/printers/mk4/storage", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let storage: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(storage[0]["path"], "/usb/");
    assert_eq!(storage[0]["kind"], "USB");
    assert_eq!(storage[0]["available"], true);

    let (status, _) = send(&app, &state, Method::DELETE, "/api/printers/mk4", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send(&app, &state, Method::GET, "/api/printers/mk4", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rejects_invalid_requests() {
    let (app, state) = common::test_app();

    let (status, _) = send(
        &app,
        &state,
        Method::POST,
        "/api/printers",
        Some(registration("MK4", "127.0.0.1:1")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send(
        &app,
        &state,
        Method::POST,
        "/api/printers",
        Some(registration("mk4", "http://127.0.0.1:1")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send(&app, &state, Method::GET, "/api/printers/ghost/info", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = send(&app, &state, Method::DELETE, "/api/printers/ghost", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unreachable_printer_is_not_registered() {
    let (app, state) = common::test_app();

    let (status, body) = send(
        &app,
        &state,
        Method::POST,
        "/api/printers",
        Some(registration("mk4", "127.0.0.1:1")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");

    let (status, _) = send(&app, &state, Method::GET, "/api/printers/mk4", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn duplicate_names_are_rejected() {
    let printer = fake_printer().await;
    let (app, state) = common::test_app();

    let (status, _) = send(
        &app,
        &state,
        Method::POST,
        "/api/printers",
        Some(registration("mk4", &printer.to_string())),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = send(
        &app,
        &state,
        Method::POST,
        "/api/printers",
        Some(registration("mk4", &printer.to_string())),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}
