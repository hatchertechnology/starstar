//! File metadata, listings, HEAD, printing and deletion.

use axum::body::Body;
use axum::http::{Method, StatusCode};
use serde_json::json;
use starstar_shared::Role;

mod common;
mod fake_prusalink;

use fake_prusalink::{Canned, TestApi};

#[tokio::test]
async fn lists_storage_root() {
    let api = TestApi::start().await;
    let r = api.get(Role::Viewer, "/api/printers/mk4/files/usb").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let j = r.json();
    assert_eq!(j["kind"], "folder");
    assert_eq!(j["children"][0]["kind"], "print_file");
    assert_eq!(j["children"][0]["display_name"], "cube-12mm.bgcode");
    assert_eq!(j["children"][1]["kind"], "folder");
    assert!(
        api.fake
            .authorized(Method::GET)
            .iter()
            .any(|q| q.uri == "/api/v1/files/usb/")
    );
}

#[tokio::test]
async fn shows_print_file_metadata() {
    let api = TestApi::start().await;
    let r = api
        .get(Role::Viewer, "/api/printers/mk4/files/usb/cube-12mm.bgcode")
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let j = r.json();
    assert_eq!(j["kind"], "print_file");
    assert_eq!(j["size"], 123456);
    assert_eq!(j["modified"], 1700000000);
    assert_eq!(j["meta"]["filament_type"], "PLA");
    assert_eq!(j["meta"]["nozzle_temp"], 215.0);
    assert_eq!(j["meta"]["filament_used_mm"], 1040.5);
    assert!(j.get("children").is_none());
}

#[tokio::test]
async fn head_reports_file_state() {
    let api = TestApi::start().await;
    let r = api
        .call(
            Role::Viewer,
            Method::HEAD,
            "/api/printers/mk4/files/usb/cube-12mm.bgcode",
            &[],
            Body::empty(),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.headers["currently-printed"], "true");
    assert_eq!(r.headers["read-only"], "false");
    assert!(r.body.is_empty());
}

#[tokio::test]
async fn missing_file_is_404() {
    let api = TestApi::start().await;
    let r = api
        .get(Role::Viewer, "/api/printers/mk4/files/usb/nope.bgcode")
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert!(r.json()["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn unknown_storage_is_404() {
    let api = TestApi::start().await;
    let r = api
        .get(Role::Viewer, "/api/printers/mk4/files/sd/cube-12mm.bgcode")
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert!(r.json()["error"].as_str().unwrap().contains("usb"));
    assert!(
        !api.fake
            .requests()
            .iter()
            .any(|q| q.uri.starts_with("/api/v1/files/"))
    );
}

#[tokio::test]
async fn file_paths_round_trip_encoded() {
    let api = TestApi::start().await;
    let up = "/api/v1/files/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode";
    api.fake.set(
        Method::GET,
        up,
        Canned::json(
            200,
            json!({"name":"café #1?%.bgcode","type":"PRINT_FILE","read_only":false,"m_timestamp":1}),
        ),
    );
    let r = api
        .get(
            Role::Viewer,
            "/api/printers/mk4/files/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode",
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(r.json()["name"], "café #1?%.bgcode");
    // The fake verifies the digest against the encoded uri, so this proves they match.
    assert!(api.fake.authorized(Method::GET).iter().any(|q| q.uri == up));
}

#[tokio::test]
async fn unsafe_paths_refused_without_contacting_printer() {
    let api = TestApi::start().await;
    for tail in ["a/%2E%2E/b", "a//b", "a%00b", "a%5Cb"] {
        let r = api
            .get(Role::Viewer, &format!("/api/printers/mk4/files/usb/{tail}"))
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{tail}: {}", r.text());
        assert!(r.json()["error"].as_str().unwrap().contains("file paths"));
    }
    assert!(api.fake.requests().is_empty());
}

async fn post(api: &TestApi, uri: &str) -> fake_prusalink::TestResponse {
    api.call(Role::Operator, Method::POST, uri, &[], Body::empty())
        .await
}

#[tokio::test]
async fn starts_print() {
    let api = TestApi::start().await;
    let r = post(&api, "/api/printers/mk4/files/usb/cube-12mm.bgcode/print").await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.text());
    assert!(
        api.fake
            .authorized(Method::POST)
            .iter()
            .any(|q| q.uri == "/api/v1/files/usb/cube-12mm.bgcode")
    );
}

#[tokio::test]
async fn print_while_busy_is_409() {
    let api = TestApi::start().await;
    api.fake.set(
        Method::POST,
        "/api/v1/files/usb/cube-12mm.bgcode",
        Canned::json(409, json!({"title":"Conflict","text":"Printer is busy"})),
    );
    let r = post(&api, "/api/printers/mk4/files/usb/cube-12mm.bgcode/print").await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert!(r.json()["error"].as_str().unwrap().contains("busy"));
}

#[tokio::test]
async fn post_without_print_suffix_is_404() {
    let api = TestApi::start().await;
    let r = post(&api, "/api/printers/mk4/files/usb/cube-12mm.bgcode").await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert!(
        r.json()["error"]
            .as_str()
            .unwrap()
            .contains("unknown file action")
    );
    assert!(api.fake.requests().is_empty());
}

#[tokio::test]
async fn deletes_file_with_force_header() {
    let api = TestApi::start().await;
    for (query, expected) in [("?force=true", "?1"), ("", "?0")] {
        let r = api
            .call(
                Role::Admin,
                Method::DELETE,
                &format!("/api/printers/mk4/files/usb/parts{query}"),
                &[],
                Body::empty(),
            )
            .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.text());
        let sent = api.fake.authorized(Method::DELETE);
        let last = sent.last().unwrap();
        assert_eq!(last.uri, "/api/v1/files/usb/parts");
        assert_eq!(last.headers["force"], expected);
    }
}

#[tokio::test]
async fn deleting_printed_file_is_409() {
    let api = TestApi::start().await;
    api.fake.set(
        Method::DELETE,
        "/api/v1/files/usb/cube-12mm.bgcode",
        Canned::json(
            409,
            json!({"title":"Conflict","text":"File is being printed"}),
        ),
    );
    let r = api
        .call(
            Role::Admin,
            Method::DELETE,
            "/api/printers/mk4/files/usb/cube-12mm.bgcode",
            &[],
            Body::empty(),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert!(
        r.json()["error"]
            .as_str()
            .unwrap()
            .contains("being printed")
    );
}
