//! Camera endpoints.

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use starstar_shared::Role;

mod common;
mod fake_prusalink;

use fake_prusalink::{Canned, TestApi};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake";

fn assert_cam1(list: &Value) {
    assert_eq!(list[0]["id"], "cam1");
    assert_eq!(list[0]["name"], "Buddy3D");
    assert_eq!(list[0]["driver"], "V4L2");
    assert_eq!(list[0]["resolution"], "1280x720");
    assert_eq!(list[0]["connected"], true);
    assert_eq!(list[0]["linked"], false);
}

#[tokio::test]
async fn lists_cameras() {
    let api = TestApi::start().await;
    let r = api.get(Role::Viewer, "/api/printers/mk4/cameras").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_cam1(&r.json());
}

#[tokio::test]
async fn lists_wrapped_camera_list() {
    let api = TestApi::start().await;
    api.fake.set(
        Method::GET,
        "/api/v1/cameras",
        Canned::json(
            200,
            json!({"camera_list":[{"camera_id":"cam1","config":{"name":"Buddy3D","driver":"V4L2","path":"/dev/video0","resolution":"1280x720"},"connected":true,"detected":true,"stored":true,"linked":false}]}),
        ),
    );
    let r = api.get(Role::Viewer, "/api/printers/mk4/cameras").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_cam1(&r.json());
}

#[tokio::test]
async fn shows_camera_detail() {
    let api = TestApi::start().await;
    let r = api
        .get(Role::Viewer, "/api/printers/mk4/cameras/cam1")
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let j = r.json();
    assert_eq!(j["name"], "Buddy3D");
    assert_eq!(j["trigger_scheme"], "thirty_sec");
    assert_eq!(j["resolution"]["width"], 1280);
    assert_eq!(j["available_resolutions"][0]["height"], 720);
    assert_eq!(j["capabilities"], json!(["trigger_scheme", "resolution"]));
}

#[tokio::test]
async fn default_snapshot_is_png() {
    let api = TestApi::start().await;
    let r = api
        .get(Role::Viewer, "/api/printers/mk4/cameras/snap")
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(r.headers["content-type"], "image/png");
    assert_eq!(r.headers["cache-control"], "no-store");
    assert_eq!(r.body.as_ref(), PNG);
    let uris: Vec<String> = api.fake.requests().into_iter().map(|q| q.uri).collect();
    assert!(uris.contains(&"/api/v1/cameras/snap".to_owned()));
    assert!(!uris.contains(&"/api/v1/cameras/cam1".to_owned()));
}

#[tokio::test]
async fn camera_snapshot_by_id() {
    let api = TestApi::start().await;
    let r = api
        .get(Role::Viewer, "/api/printers/mk4/cameras/cam1/snap")
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(r.headers["content-type"], "image/png");
    assert_eq!(r.body.as_ref(), PNG);
    assert!(
        api.fake
            .authorized(Method::GET)
            .iter()
            .any(|q| q.uri == "/api/v1/cameras/cam1/snap")
    );
}

#[tokio::test]
async fn no_snapshot_yet_is_204() {
    let api = TestApi::start().await;
    api.fake
        .set(Method::GET, "/api/v1/cameras/snap", Canned::empty(204));
    let r = api
        .get(Role::Viewer, "/api/printers/mk4/cameras/snap")
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert!(r.body.is_empty());
}

#[tokio::test]
async fn cameras_unavailable_is_502() {
    let api = TestApi::start().await;
    api.fake
        .set(Method::GET, "/api/v1/cameras", Canned::empty(503));
    let r = api.get(Role::Viewer, "/api/printers/mk4/cameras").await;
    assert_eq!(r.status, StatusCode::BAD_GATEWAY);
    assert!(r.json()["error"].as_str().unwrap().contains("unavailable"));
}

#[tokio::test]
async fn camera_forbidden_upstream_is_403() {
    let api = TestApi::start().await;
    api.fake.set(
        Method::GET,
        "/api/v1/cameras/cam1",
        Canned::json(403, json!({"title":"Forbidden","text":"camera is locked"})),
    );
    let r = api
        .get(Role::Viewer, "/api/printers/mk4/cameras/cam1")
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert!(
        r.json()["error"]
            .as_str()
            .unwrap()
            .contains("camera is locked")
    );
}

#[tokio::test]
async fn bad_camera_id_is_400() {
    let api = TestApi::start().await;
    let r = api
        .get(Role::Viewer, "/api/printers/mk4/cameras/a%20b")
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text());
    assert!(r.json()["error"].as_str().unwrap().contains("camera id"));
    assert!(api.fake.requests().is_empty());
}

#[tokio::test]
async fn orders_cameras() {
    let api = TestApi::start().await;
    let r = api
        .send_json(
            Role::Admin,
            Method::PUT,
            "/api/printers/mk4/cameras/order",
            json!(["cam2", "cam1"]),
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.text());
    let sent = api.fake.authorized(Method::PUT);
    let sent = sent.iter().find(|q| q.uri == "/api/v1/cameras").unwrap();
    let body: Value = serde_json::from_slice(&sent.body).unwrap();
    assert_eq!(body, json!(["cam2", "cam1"]));
}

#[tokio::test]
async fn order_rejects_bad_ids() {
    let api = TestApi::start().await;
    let r = api
        .send_json(
            Role::Admin,
            Method::PUT,
            "/api/printers/mk4/cameras/order",
            json!(["cam1", "a/b"]),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert!(api.fake.requests().is_empty());
}

#[tokio::test]
async fn patches_camera_config_in_prusalink_shape() {
    let api = TestApi::start().await;
    let r = api
        .send_json(
            Role::Admin,
            Method::PATCH,
            "/api/printers/mk4/cameras/cam1/config",
            json!({"trigger_scheme":"each_layer","resolution":{"width":640,"height":480}}),
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.text());
    let sent = api.fake.authorized(Method::PATCH);
    assert_eq!(sent[0].uri, "/api/v1/cameras/cam1/config");
    assert_eq!(sent[0].headers["content-type"], "application/json");
    let body: Value = serde_json::from_slice(&sent[0].body).unwrap();
    assert_eq!(
        body,
        json!({"trigger_scheme":"EACH_LAYER","resolution":{"width":640,"height":480}})
    );
}

#[tokio::test]
async fn camera_admin_calls_forward() {
    let api = TestApi::start().await;
    for (method, uri, up_uri) in [
        (
            Method::POST,
            "/api/printers/mk4/cameras/cam1",
            "/api/v1/cameras/cam1",
        ),
        (
            Method::DELETE,
            "/api/printers/mk4/cameras/cam1",
            "/api/v1/cameras/cam1",
        ),
        (
            Method::DELETE,
            "/api/printers/mk4/cameras/cam1/config",
            "/api/v1/cameras/cam1/config",
        ),
        (
            Method::POST,
            "/api/printers/mk4/cameras/cam1/connection",
            "/api/v1/cameras/cam1/connection",
        ),
        (
            Method::DELETE,
            "/api/printers/mk4/cameras/cam1/connection",
            "/api/v1/cameras/cam1/connection",
        ),
    ] {
        let r = if method == Method::POST && up_uri == "/api/v1/cameras/cam1" {
            api.send_json(Role::Admin, method.clone(), uri, json!({"name":"Buddy3D"}))
                .await
        } else {
            api.call(
                Role::Admin,
                method.clone(),
                uri,
                &[],
                axum::body::Body::empty(),
            )
            .await
        };
        assert_eq!(
            r.status,
            StatusCode::NO_CONTENT,
            "{method} {uri}: {}",
            r.text()
        );
        assert!(
            api.fake
                .authorized(method.clone())
                .iter()
                .any(|q| q.uri == up_uri),
            "{method} {up_uri}"
        );
    }
}

#[tokio::test]
async fn takes_snapshot() {
    let api = TestApi::start().await;
    let r = api
        .call(
            Role::Operator,
            Method::POST,
            "/api/printers/mk4/cameras/cam1/snap",
            &[],
            axum::body::Body::empty(),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(r.headers["content-type"], "image/png");
    assert_eq!(r.body.as_ref(), PNG);
    let sent = api.fake.authorized(Method::POST);
    assert_eq!(sent[0].body, b"{}");
}

#[tokio::test]
async fn snapshot_timeout_upstream_is_504() {
    let api = TestApi::start().await;
    api.fake.set(
        Method::POST,
        "/api/v1/cameras/cam1/snap",
        Canned::json(408, json!({"title":"Timeout","text":"camera busy"})),
    );
    let r = api
        .call(
            Role::Operator,
            Method::POST,
            "/api/printers/mk4/cameras/cam1/snap",
            &[],
            axum::body::Body::empty(),
        )
        .await;
    assert_eq!(r.status, StatusCode::GATEWAY_TIMEOUT);
    assert!(r.json()["error"].as_str().unwrap().contains("camera busy"));
}

#[tokio::test]
async fn register_conflict_is_409() {
    let api = TestApi::start().await;
    api.fake.set(
        Method::POST,
        "/api/v1/cameras/cam1",
        Canned::json(409, json!({"title":"Conflict","text":"already registered"})),
    );
    let r = api
        .send_json(
            Role::Admin,
            Method::POST,
            "/api/printers/mk4/cameras/cam1",
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert!(
        r.json()["error"]
            .as_str()
            .unwrap()
            .contains("already registered")
    );
}
