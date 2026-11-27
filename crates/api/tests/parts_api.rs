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
    let duplicate = upload(format!("/api/parts/{id}/files?name=cube.step"), b"STEP").await;
    assert_eq!(duplicate.status, 200);
    assert_eq!(duplicate.json()["id"], source.json()["id"]);
    assert_eq!(
        api.call(
            Role::Operator,
            Method::POST,
            &format!("/api/parts/{id}/files?name=x.step"),
            &[],
            Body::from("x")
        )
        .await
        .status,
        411
    );
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
    assert_eq!(tree["setups"][0]["revisions"][0]["number"], 1);
    assert_eq!(tree["setups"][0]["revisions"][0]["slices"][0]["id"], fid);
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
    assert_eq!(
        preview.headers["content-security-policy"],
        "default-src 'none'; sandbox; frame-ancestors 'none'"
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
    assert_eq!(download.headers["content-type"], "application/octet-stream");
    assert_eq!(download.headers["x-content-type-options"], "nosniff");
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
    tokio::fs::remove_file(
        api.state
            .parts
            .blobs
            .preview(file["sha256"].as_str().unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(
        api.get(Role::Viewer, &format!("/api/files/{fid}/preview"))
            .await
            .status,
        500
    );
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

#[tokio::test]
async fn upload_limit_and_missing_asset() {
    let api = TestApi::start_with(starstar_api::config::Config {
        max_upload_bytes: 4,
        ..Default::default()
    })
    .await;
    let part = api
        .send_json(
            Role::Operator,
            Method::POST,
            "/api/parts",
            serde_json::json!({"name":"p"}),
        )
        .await
        .json();
    let id = part["id"].as_i64().unwrap();
    let path = format!("/api/parts/{id}/files?name=a.step");
    assert_eq!(
        api.call(
            Role::Operator,
            Method::POST,
            &path,
            &[("content-length", "5")],
            Body::from("12345")
        )
        .await
        .status,
        413
    );
    let file = api
        .call(
            Role::Operator,
            Method::POST,
            &path,
            &[("content-length", "4")],
            Body::from("1234"),
        )
        .await
        .json();
    let fid = file["id"].as_i64().unwrap();
    tokio::fs::remove_file(api.state.parts.blobs.path(file["sha256"].as_str().unwrap()))
        .await
        .unwrap();
    assert_eq!(
        api.get(Role::Viewer, &format!("/api/files/{fid}/download"))
            .await
            .status,
        500
    );
}

#[tokio::test]
async fn upload_uses_latest_model_after_streaming() {
    let api = TestApi::start().await;
    let part = api
        .send_json(
            Role::Operator,
            Method::POST,
            "/api/parts",
            serde_json::json!({"name":"p"}),
        )
        .await
        .json();
    let id = part["id"].as_i64().unwrap();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
    let stream = futures_util::stream::once(async move {
        started_tx.send(()).unwrap();
        resume_rx.await.unwrap();
        Ok::<_, std::io::Error>(axum::body::Bytes::from_static(b"STEP"))
    });
    let path = format!("/api/parts/{id}/files?name=a.step");
    let upload = api.call(
        Role::Operator,
        Method::POST,
        &path,
        &[("content-length", "4")],
        Body::from_stream(stream),
    );
    let revision = async {
        started_rx.await.unwrap();
        let r = api
            .send_json(
                Role::Operator,
                Method::POST,
                &format!("/api/parts/{id}/revisions"),
                serde_json::json!({"carry":[]}),
            )
            .await;
        assert_eq!(r.status, 201);
        let revision = r.json()["id"].as_i64().unwrap();
        resume_tx.send(()).unwrap();
        revision
    };
    let (file, revision) = tokio::join!(upload, revision);
    assert_eq!(file.status, 201);
    assert_eq!(file.json()["model_revision_id"], revision);
}
