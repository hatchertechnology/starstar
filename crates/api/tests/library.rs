use axum::{
    body::Body,
    http::{Method, StatusCode},
};
use starstar_shared::Role;
mod common;
mod fake_prusalink;
use fake_prusalink::{TestApi, TestResponse};
const CUBE: &[u8] = include_bytes!("../../../prints/cube-12mm/cube-12mm.MK4.bgcode");
async fn upload(api: &TestApi, name: &str, data: &'static [u8]) -> TestResponse {
    api.call(
        Role::Operator,
        Method::POST,
        &format!("/api/library?name={name}"),
        &[("content-length", &data.len().to_string())],
        Body::from(data),
    )
    .await
}
async fn get(api: &TestApi, path: &str) -> TestResponse {
    api.call(Role::Viewer, Method::GET, path, &[], Body::empty())
        .await
}
#[tokio::test]
async fn upload_list_get_roundtrip() {
    let api = TestApi::start().await;
    let r = upload(&api, "cube-12mm.MK4.bgcode", CUBE).await;
    assert_eq!(r.status, 201);
    let f = r.json();
    assert_eq!(f["version"], 1);
    assert_eq!(f["meta"]["printer_model"], "MK4IS");
    assert_eq!(f["has_preview"], true);
    assert_eq!(get(&api, "/api/library").await.json()[0], f);
    assert_eq!(
        get(&api, &format!("/api/library/{}", f["id"])).await.json(),
        f
    );
    let again = upload(&api, "cube-12mm.MK4.bgcode", CUBE).await;
    assert_eq!(again.status, 200);
    assert_eq!(again.json()["id"], f["id"]);
    let p = get(&api, &format!("/api/library/{}/preview", f["id"])).await;
    assert_eq!(p.headers["content-type"], "image/png");
    assert_eq!(p.headers["cache-control"], "private, max-age=300");
    assert_eq!(p.headers["x-content-type-options"], "nosniff");
    assert!(
        p.headers["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("sandbox")
    );
    assert!(p.body.starts_with(b"\x89PNG"));
    let d = get(&api, &format!("/api/library/{}/download", f["id"])).await;
    assert_eq!(d.body, CUBE);
    assert_eq!(
        d.headers["content-disposition"],
        "attachment; filename*=UTF-8''cube-12mm.MK4.bgcode"
    );
    let path = format!("/api/library/{}", f["id"]);
    assert_eq!(
        api.call(Role::Operator, Method::DELETE, &path, &[], Body::empty())
            .await
            .status,
        403
    );
    assert_eq!(
        api.call(Role::Admin, Method::DELETE, &path, &[], Body::empty())
            .await
            .status,
        204
    );
    assert_eq!(get(&api, &path).await.status, 404);
}
#[tokio::test]
async fn invalid_and_short_uploads() {
    let api = TestApi::start_with(starstar_api::config::Config {
        max_upload_bytes: 1000,
        ..Default::default()
    })
    .await;
    // Discover the isolated test store root from a temporary catalog entry.
    let seed = upload(&api, "seed.gcode", b"G28").await;
    let seed_id = seed.json()["id"].as_i64().unwrap();
    let seed_file = api.state.library.get(seed_id).await.unwrap().unwrap();
    let blob = api.state.library.blob_path(&seed_file);
    let root = blob.ancestors().nth(4).unwrap().to_owned();
    api.state.library.delete(seed_id).await.unwrap();
    let bad = upload(&api, "part.stl", b"x").await;
    assert_eq!(bad.status, 400);
    assert_eq!(
        bad.json()["error"],
        "only .bgcode and .gcode files can be added to the library"
    );
    assert_eq!(
        api.call(
            Role::Operator,
            Method::POST,
            "/api/library?name=a.gcode",
            &[],
            Body::from("x")
        )
        .await
        .status,
        411
    );
    assert_eq!(
        api.call(
            Role::Operator,
            Method::POST,
            "/api/library?name=a.gcode",
            &[("content-length", "2000")],
            Body::from(vec![0; 2000])
        )
        .await
        .status,
        413
    );
    assert_eq!(
        api.call(
            Role::Operator,
            Method::POST,
            "/api/library?name=a.gcode",
            &[("content-length", "500")],
            Body::from("1234567890")
        )
        .await
        .status,
        400
    );
    assert_eq!(
        get(&api, "/api/library").await.json(),
        serde_json::json!([])
    );
    assert_eq!(std::fs::read_dir(root.join("tmp")).unwrap().count(), 0);
    assert!(files_under(&root.join("blobs")).is_empty());
    assert_eq!(
        get(&api, "/api/library/not-a-number").await.status,
        StatusCode::BAD_REQUEST
    );
}
#[tokio::test]
async fn hostile_names_stay_display_names() {
    let api = TestApi::start().await;
    let mut root = None;
    for name in [
        "..%2F..%2Fetc%2Fx.gcode",
        "a%2Fb.bgcode",
        "caf%C3%A9%20%231%3F%25.bgcode",
    ] {
        let response = upload(&api, name, b"G28\n").await;
        assert_eq!(response.status, 201);
        let id = response.json()["id"].as_i64().unwrap();
        let file = api.state.library.get(id).await.unwrap().unwrap();
        root = Some(
            api.state
                .library
                .blob_path(&file)
                .ancestors()
                .nth(4)
                .unwrap()
                .to_owned(),
        );
    }
    let root = root.unwrap();
    let files = files_under(&root);
    assert!(!files.is_empty());
    assert!(
        files
            .iter()
            .all(|path| path.starts_with(root.join("blobs"))
                || path.starts_with(root.join("thumbs")))
    );
    let list = get(&api, "/api/library").await.json();
    let names: Vec<_> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    for expected in ["../../etc/x.gcode", "a/b.bgcode", "café #1?%.bgcode"] {
        assert!(names.contains(&expected));
    }
}

fn files_under(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(files_under(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[tokio::test]
async fn missing_referenced_assets_are_server_errors() {
    let api = TestApi::start().await;
    let response = upload(&api, "cube.bgcode", CUBE).await;
    let id = response.json()["id"].as_i64().unwrap();
    let file = api.state.library.get(id).await.unwrap().unwrap();
    std::fs::remove_file(api.state.library.preview_path(&file).unwrap()).unwrap();
    std::fs::remove_file(api.state.library.blob_path(&file)).unwrap();
    for asset in ["preview", "download"] {
        assert_eq!(
            get(&api, &format!("/api/library/{id}/{asset}"))
                .await
                .status,
            500
        );
    }
    api.state.library.delete(id).await.unwrap();
    for asset in ["preview", "download"] {
        assert_eq!(
            get(&api, &format!("/api/library/{id}/{asset}"))
                .await
                .status,
            404
        );
    }
}
