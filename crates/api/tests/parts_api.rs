use axum::{
    body::Body,
    http::{Method, StatusCode},
};
use starstar_shared::Role;
mod common;
mod fake_prusalink;
use fake_prusalink::TestApi;

#[tokio::test]
async fn creates_parts_and_retires_library() {
    let api = TestApi::start().await;
    let response = api
        .call(
            Role::Operator,
            Method::POST,
            "/api/parts",
            &[("content-type", "application/json")],
            Body::from(r#"{"name":"Cube"}"#),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED);
    assert_eq!(response.json()["name"], "Cube");
    assert_eq!(
        api.call(
            Role::Viewer,
            Method::GET,
            "/api/library",
            &[],
            Body::empty()
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn upload_tree_preview_download_and_match() {
    let api = TestApi::start().await;
    let json_headers = [("content-type", "application/json")];
    let part = api
        .call(
            Role::Operator,
            Method::POST,
            "/api/parts",
            &json_headers,
            Body::from(r#"{"name":"Cube"}"#),
        )
        .await
        .json();
    let id = part["id"].as_i64().unwrap();
    let api_ref = &api;
    let upload = |path: String, data: &'static [u8]| async move {
        api_ref
            .call(
                Role::Operator,
                Method::POST,
                &path,
                &[("content-length", &data.len().to_string())],
                Body::from(data),
            )
            .await
    };
    let source = upload(format!("/api/parts/{id}/files?name=cube.step"), b"STEP").await;
    assert_eq!(source.status, 201);
    let setup = api
        .call(
            Role::Operator,
            Method::POST,
            &format!("/api/parts/{id}/setups"),
            &json_headers,
            Body::from(r#"{"name":"Plate"}"#),
        )
        .await
        .json();
    let sid = setup["id"].as_i64().unwrap();
    let data: &'static [u8] = include_bytes!("../../../prints/cube-12mm/cube-12mm.MK4.bgcode");
    let slice = upload(
        format!("/api/parts/{id}/files?name=cube.bgcode&setup={sid}&profile=speed"),
        data,
    )
    .await;
    assert_eq!(slice.status, 201);
    let file = slice.json();
    let fid = file["id"].as_i64().unwrap();
    let tree = api
        .call(
            Role::Viewer,
            Method::GET,
            &format!("/api/parts/{id}"),
            &[],
            Body::empty(),
        )
        .await
        .json();
    assert_eq!(
        tree["setups"][0]["revisions"][0]["slices"][0]["target"]["profile"],
        "speed"
    );
    let preview = api
        .call(
            Role::Viewer,
            Method::GET,
            &format!("/api/files/{fid}/preview"),
            &[],
            Body::empty(),
        )
        .await;
    assert_eq!(preview.status, 200);
    assert_eq!(preview.headers["content-type"], "image/png");
    assert_eq!(preview.headers["cache-control"], "private, max-age=300");
    assert_eq!(preview.headers["x-content-type-options"], "nosniff");
    assert!(
        preview.headers["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("sandbox")
    );
    let download = api
        .call(
            Role::Viewer,
            Method::GET,
            &format!("/api/files/{fid}/download"),
            &[],
            Body::empty(),
        )
        .await;
    assert_eq!(download.body, data);
    assert_eq!(
        download.headers["content-disposition"],
        "attachment; filename*=UTF-8''cube.bgcode"
    );
    let result = api
        .call(
            Role::Operator,
            Method::GET,
            &format!("/api/parts/{id}/match?setup={sid}&profile=speed"),
            &[],
            Body::empty(),
        )
        .await;
    assert_eq!(result.status, 200);
    assert_eq!(result.json()[0]["slice"]["id"], fid);
}

#[tokio::test]
async fn other_download_and_unreachable_printer() {
    let api = TestApi::start().await;
    api.add_printer("offline", "127.0.0.1:1", fake_prusalink::PASSWORD)
        .await;
    let p = api
        .call(
            Role::Operator,
            Method::POST,
            "/api/parts",
            &[("content-type", "application/json")],
            Body::from(r#"{"name":"Other"}"#),
        )
        .await
        .json();
    let id = p["id"].as_i64().unwrap();
    let file = api
        .call(
            Role::Operator,
            Method::POST,
            &format!("/api/parts/{id}/files?role=other&name=note.txt"),
            &[("content-length", "4")],
            Body::from("test"),
        )
        .await
        .json();
    let fid = file["id"].as_i64().unwrap();
    assert_eq!(
        api.call(
            Role::Viewer,
            Method::GET,
            &format!("/api/files/{fid}/preview"),
            &[],
            Body::empty()
        )
        .await
        .status,
        404
    );
    let d = api
        .call(
            Role::Viewer,
            Method::GET,
            &format!("/api/files/{fid}/download"),
            &[],
            Body::empty(),
        )
        .await;
    assert_eq!(
        d.headers["content-disposition"],
        "attachment; filename*=UTF-8''note.txt"
    );
    let result = api
        .call(
            Role::Operator,
            Method::GET,
            &format!("/api/parts/{id}/match"),
            &[],
            Body::empty(),
        )
        .await
        .json();
    let offline = result
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["printer"] == "offline")
        .unwrap();
    assert_eq!(offline["blocked"], "printer unreachable");
}
