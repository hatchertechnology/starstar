//! Streamed uploads to the printer.

use axum::body::{Body, Bytes};
use axum::http::{Method, StatusCode};
use serde_json::json;
use starstar_api::config::Config;
use starstar_shared::Role;

mod common;
mod fake_prusalink;

use fake_prusalink::{Canned, TestApi, TestResponse};

fn chunked_body(chunks: usize, size: usize) -> (Body, Vec<u8>) {
    let parts: Vec<Vec<u8>> = (0..chunks)
        .map(|i| vec![b'A' + (i % 26) as u8; size])
        .collect();
    let all = parts.concat();
    let stream = futures_util::stream::iter(
        parts
            .into_iter()
            .map(|p| Ok::<_, std::io::Error>(Bytes::from(p))),
    );
    (Body::from_stream(stream), all)
}

async fn put(api: &TestApi, uri: &str, length: Option<usize>, body: Body) -> TestResponse {
    let length = length.map(|l| l.to_string());
    let mut headers = Vec::new();
    if let Some(l) = &length {
        headers.push(("content-length", l.as_str()));
    }
    api.call(Role::Operator, Method::PUT, uri, &headers, body)
        .await
}

#[tokio::test]
async fn uploads_multi_chunk_body_streamed_with_length() {
    let api = TestApi::start().await;
    let (body, all) = chunked_body(48, 64 * 1024);
    assert_eq!(all.len(), 3_145_728);
    let r = put(
        &api,
        "/api/printers/mk4/files/usb/big.bgcode?overwrite=true&print=true",
        Some(all.len()),
        body,
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());

    let puts = api.fake.authorized(Method::PUT);
    assert_eq!(puts.len(), 1);
    let sent = &puts[0];
    assert_eq!(sent.uri, "/api/v1/files/usb/big.bgcode");
    assert!(sent.body == all, "body arrived intact");
    assert_eq!(sent.headers["content-length"], "3145728");
    assert_eq!(sent.headers["overwrite"], "?1");
    assert_eq!(sent.headers["print-after-upload"], "?1");
    assert!(sent.headers.get("transfer-encoding").is_none());

    // Exactly one PUT overall, preceded by an unauthenticated HEAD probe.
    let all_requests = api.fake.requests();
    let put_at = all_requests
        .iter()
        .position(|q| q.method == Method::PUT)
        .unwrap();
    assert_eq!(
        all_requests
            .iter()
            .filter(|q| q.method == Method::PUT)
            .count(),
        1
    );
    let head = all_requests[..put_at]
        .iter()
        .find(|q| q.method == Method::HEAD)
        .expect("HEAD probe before the PUT");
    assert_eq!(head.uri, "/api/v1/files/usb/big.bgcode");
    assert!(!head.authorized);
}

#[tokio::test]
async fn upload_defaults_are_no_overwrite_no_print() {
    let api = TestApi::start().await;
    let r = put(
        &api,
        "/api/printers/mk4/files/usb/a.bgcode",
        Some(3),
        Body::from("abc"),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    let sent = &api.fake.authorized(Method::PUT)[0];
    assert_eq!(sent.headers["overwrite"], "?0");
    assert_eq!(sent.headers["print-after-upload"], "?0");
    assert_eq!(sent.body, b"abc");
}

#[tokio::test]
async fn upload_with_reserved_characters_is_authenticated() {
    let api = TestApi::start().await;
    let r = put(
        &api,
        "/api/printers/mk4/files/usb/My%20Parts/caf%C3%A9%20%231.bgcode",
        Some(3),
        Body::from("abc"),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    let puts = api.fake.authorized(Method::PUT);
    assert_eq!(puts.len(), 1);
    assert_eq!(
        puts[0].uri,
        "/api/v1/files/usb/My%20Parts/caf%C3%A9%20%231.bgcode"
    );
}

#[tokio::test]
async fn upload_over_limit_is_413_before_contacting_printer() {
    let api = TestApi::start_with(Config {
        max_upload_bytes: 1024,
        ..Config::default()
    })
    .await;
    let r = put(
        &api,
        "/api/printers/mk4/files/usb/a.bgcode",
        Some(2048),
        Body::from(vec![0u8; 2048]),
    )
    .await;
    assert_eq!(r.status, StatusCode::PAYLOAD_TOO_LARGE, "{}", r.text());
    assert!(r.json()["error"].as_str().unwrap().contains("1024 bytes"));
    assert!(api.fake.requests().is_empty());
}

#[tokio::test]
async fn upload_without_content_length_is_411() {
    let api = TestApi::start().await;
    let r = put(
        &api,
        "/api/printers/mk4/files/usb/a.bgcode",
        None,
        Body::from("abc"),
    )
    .await;
    assert_eq!(r.status, StatusCode::LENGTH_REQUIRED, "{}", r.text());
    assert!(
        r.json()["error"]
            .as_str()
            .unwrap()
            .contains("Content-Length")
    );
    assert!(api.fake.requests().is_empty());
}

#[tokio::test]
async fn upload_to_folder_path_is_400() {
    let api = TestApi::start().await;
    let r = put(
        &api,
        "/api/printers/mk4/files/usb/parts/",
        Some(3),
        Body::from("abc"),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text());
    assert!(
        r.json()["error"]
            .as_str()
            .unwrap()
            .contains("empty segments")
    );
    assert!(api.fake.requests().is_empty());
}

#[tokio::test]
async fn upload_existing_file_is_409() {
    let api = TestApi::start().await;
    api.fake.set(
        Method::PUT,
        "/api/v1/files/usb/cube-12mm.bgcode",
        Canned::json(
            409,
            json!({"title":"Conflict","text":"File already exists"}),
        ),
    );
    let r = put(
        &api,
        "/api/printers/mk4/files/usb/cube-12mm.bgcode",
        Some(3),
        Body::from("abc"),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text());
    assert!(
        r.json()["error"]
            .as_str()
            .unwrap()
            .contains("already exists")
    );
}

#[tokio::test]
async fn upload_with_wrong_printer_password_fails_before_any_put() {
    // The storage lookup that precedes the upload already fails with the bad
    // password, so no PUT is attempted. The hand-signed PUT path is covered by
    // `client_upload_with_wrong_password_reaches_signed_put` below.
    let api = TestApi::start().await;
    api.add_printer("bad", &api.fake.addr.to_string(), "wrong")
        .await;
    let r = put(
        &api,
        "/api/printers/bad/files/usb/a.bgcode",
        Some(3),
        Body::from("abc"),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_GATEWAY, "{}", r.text());
    let error = r.json()["error"].as_str().unwrap().to_owned();
    assert!(error.contains("stored credentials"));
    assert!(!error.contains("wrong"));
    assert!(
        api.fake.requests().iter().all(|r| r.method != Method::PUT),
        "no PUT should have reached the printer"
    );
}

#[tokio::test]
async fn client_upload_with_wrong_password_reaches_signed_put() {
    use starstar_api::prusalink::{PrusaError, PrusaLinkClient};

    let api = TestApi::start().await;
    let client = PrusaLinkClient::new(
        reqwest::Client::new(),
        &api.fake.addr.to_string(),
        "maker",
        "wrong",
    );
    let err = client
        .upload(
            "usb",
            &["a.bgcode".to_owned()],
            3,
            reqwest::Body::from("abc"),
            false,
            false,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, PrusaError::Status { status, .. } if *status == StatusCode::UNAUTHORIZED),
        "{err}"
    );
    assert!(!err.to_string().contains("wrong"));

    let puts: Vec<_> = api
        .fake
        .requests()
        .into_iter()
        .filter(|r| r.method == Method::PUT)
        .collect();
    assert_eq!(puts.len(), 1, "exactly one PUT");
    assert!(!puts[0].authorized, "the fake rejects the bad signature");
    let auth = puts[0]
        .headers
        .get("authorization")
        .expect("the PUT carries a hand-signed Authorization header")
        .to_str()
        .unwrap();
    assert!(auth.starts_with("Digest "), "{auth}");
}
