mod common;
mod fake_prusalink;
mod mcp_support;

use std::time::Duration;

use axum::http::Method;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use fake_prusalink::{Canned, PASSWORD, TestApi};
use mcp_support::*;
use serde_json::json;
use starstar_api::config::Config;
use starstar_shared::Role;

fn text(res: &serde_json::Value) -> &str {
    res["content"][0]["text"].as_str().unwrap()
}

fn fake_saw(api: &TestApi, method: Method, uri: &str) -> bool {
    api.fake.authorized(method).iter().any(|r| r.uri == uri)
}

#[tokio::test]
async fn get_job_returns_current_job_or_null() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Operator);
    let res = call_tool(&api, &t, "get_job", json!({"printer": "mk4"})).await;
    assert_eq!(res["structuredContent"]["job"]["id"], 42);
    api.fake.set(Method::GET, "/api/v1/job", Canned::empty(204));
    let res = call_tool(&api, &t, "get_job", json!({"printer": "mk4"})).await;
    assert_eq!(res["isError"], false);
    assert!(res["structuredContent"]["job"].is_null());
}

#[tokio::test]
async fn job_controls_use_given_or_current_id() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Operator);
    let res = call_tool(&api, &t, "pause_job", json!({"printer": "mk4", "id": 42})).await;
    assert_eq!(res["isError"], false);
    assert_eq!(text(&res), "Paused job 42 on mk4.");
    assert!(fake_saw(&api, Method::PUT, "/api/v1/job/42/pause"));

    let res = call_tool(&api, &t, "resume_job", json!({"printer": "mk4"})).await;
    assert_eq!(text(&res), "Resumed job 42 on mk4.");
    assert!(fake_saw(&api, Method::GET, "/api/v1/job"));
    assert!(fake_saw(&api, Method::PUT, "/api/v1/job/42/resume"));

    let res = call_tool(&api, &t, "continue_job", json!({"printer": "mk4"})).await;
    assert_eq!(text(&res), "Continued job 42 on mk4.");
    let res = call_tool(&api, &t, "stop_job", json!({"printer": "mk4"})).await;
    assert_eq!(text(&res), "Stopped job 42 on mk4.");
    assert!(fake_saw(&api, Method::DELETE, "/api/v1/job/42"));
}

#[tokio::test]
async fn job_control_without_running_job_is_409() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Operator);
    api.fake.set(Method::GET, "/api/v1/job", Canned::empty(204));
    let res = call_tool(&api, &t, "pause_job", json!({"printer": "mk4"})).await;
    assert_eq!(res["isError"], true);
    assert_eq!(res["structuredContent"]["status"], 409);
    assert_eq!(
        res["structuredContent"]["error"],
        "no job is running on mk4"
    );
}

#[tokio::test]
async fn id_less_controls_check_the_role_before_any_lookup() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    for tool in [
        "pause_job",
        "resume_job",
        "continue_job",
        "stop_job",
        "cancel_transfer",
    ] {
        let res = call_tool(&api, &t, tool, json!({"printer": "mk4"})).await;
        assert_eq!(res["isError"], true, "{tool}");
        assert_eq!(res["structuredContent"]["status"], 403, "{tool}");
    }
    assert!(api.fake.requests().is_empty());
}

#[tokio::test]
async fn transfer_tools() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Operator);
    let res = call_tool(&api, &t, "get_transfer", json!({"printer": "mk4"})).await;
    assert_eq!(res["structuredContent"]["transfer"]["id"], 7);
    let res = call_tool(&api, &t, "cancel_transfer", json!({"printer": "mk4"})).await;
    assert_eq!(text(&res), "Cancelled transfer 7 on mk4.");
    assert!(fake_saw(&api, Method::DELETE, "/api/v1/transfer/7"));
}

#[tokio::test]
async fn transfer_control_without_transfer_is_409() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Operator);
    api.fake
        .set(Method::GET, "/api/v1/transfer", Canned::empty(204));
    let res = call_tool(&api, &t, "cancel_transfer", json!({"printer": "mk4"})).await;
    assert_eq!(res["structuredContent"]["status"], 409);
    assert_eq!(
        res["structuredContent"]["error"],
        "no transfer is running on mk4"
    );
}

#[tokio::test]
async fn printer_info_storage_and_get_printer() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    let res = call_tool(&api, &t, "printer_info", json!({"printer": "mk4"})).await;
    assert_eq!(res["structuredContent"]["info"]["name"], "mk4");
    let res = call_tool(&api, &t, "list_storage", json!({"printer": "mk4"})).await;
    assert!(res["structuredContent"]["storage"].is_array());
    let res = call_tool(&api, &t, "get_printer", json!({"printer": "mk4"})).await;
    assert_eq!(res["structuredContent"]["printer"]["name"], "mk4");
    assert!(!res.to_string().contains(PASSWORD));
}

#[tokio::test]
async fn printer_errors_become_tool_errors() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Operator);
    api.fake
        .set(Method::PUT, "/api/v1/job/42/pause", Canned::empty(409));
    let res = call_tool(&api, &t, "pause_job", json!({"printer": "mk4", "id": 42})).await;
    assert_eq!(res["isError"], true);
    assert_eq!(res["structuredContent"]["status"], 409);
    assert!(text(&res).contains("printer is busy"), "{}", text(&res));

    api.add_printer("gone", "127.0.0.1:1", PASSWORD);
    let res = call_tool(&api, &t, "printer_status", json!({"printer": "gone"})).await;
    assert_eq!(res["isError"], true);
    assert_eq!(res["structuredContent"]["status"], 502);

    let api = TestApi::start_with(Config {
        printer_timeout: Duration::from_millis(200),
        ..Default::default()
    })
    .await;
    let t = token(&api, Role::Viewer);
    api.fake.set_delay(Duration::from_secs(2));
    let res = call_tool(&api, &t, "printer_status", json!({"printer": "mk4"})).await;
    assert_eq!(res["structuredContent"]["status"], 504);
}

fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

fn wrap(encoded: &str, columns: usize) -> String {
    encoded
        .as_bytes()
        .chunks(columns)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join("\r\n")
}

fn puts(api: &TestApi) -> Vec<fake_prusalink::Recorded> {
    api.fake.authorized(Method::PUT)
}

#[tokio::test]
async fn list_files_and_file_state() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    let res = call_tool(
        &api,
        &t,
        "list_files",
        json!({"printer": "mk4", "storage": "usb"}),
    )
    .await;
    assert_eq!(res["isError"], false, "{res}");
    assert!(res["structuredContent"]["entry"]["children"].is_array());
    let res = call_tool(
        &api,
        &t,
        "list_files",
        json!({"printer": "mk4", "storage": "usb", "path": "cube-12mm.bgcode"}),
    )
    .await;
    assert_eq!(
        res["structuredContent"]["entry"]["name"],
        "cube-12mm.bgcode"
    );
    let res = call_tool(
        &api,
        &t,
        "file_state",
        json!({"printer": "mk4", "storage": "usb", "path": "cube-12mm.bgcode"}),
    )
    .await;
    assert_eq!(res["structuredContent"]["state"]["read_only"], false);
    assert_eq!(res["structuredContent"]["state"]["currently_printed"], true);

    let res = call_tool(
        &api,
        &t,
        "list_files",
        json!({"printer": "mk4", "storage": "usb", "path": "../etc"}),
    )
    .await;
    assert_eq!(res["structuredContent"]["status"], 400);
    assert!(api.fake.requests().iter().all(|r| !r.uri.contains("etc")));
}

#[tokio::test]
async fn upload_file_sends_decoded_bytes_with_options() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Operator);
    let res = call_tool(
        &api,
        &t,
        "upload_file",
        json!({"printer": "mk4", "storage": "usb", "path": "jobs/cube.bgcode",
               "content_base64": b64(b"G28\nG1 X10\n"), "overwrite": true, "print": true}),
    )
    .await;
    assert_eq!(res["isError"], false, "{res}");
    assert_eq!(
        text(&res),
        "Uploaded jobs/cube.bgcode to mk4:usb. Printing started."
    );
    let sent = puts(&api)
        .into_iter()
        .find(|r| r.uri == "/api/v1/files/usb/jobs/cube.bgcode")
        .expect("upload reached the printer");
    assert_eq!(sent.body, b"G28\nG1 X10\n");
    assert_eq!(sent.headers["overwrite"], "?1");
    assert_eq!(sent.headers["print-after-upload"], "?1");
}

#[tokio::test]
async fn upload_cap_boundary() {
    let api = TestApi::start_with(Config {
        mcp_max_upload_bytes: 16,
        ..Default::default()
    })
    .await;
    let t = token(&api, Role::Operator);
    let args = |n: usize| json!({"printer": "mk4", "storage": "usb", "path": "a.gcode", "content_base64": b64(&vec![b'x'; n])});
    let res = call_tool(&api, &t, "upload_file", args(16)).await;
    assert_eq!(res["isError"], false, "{res}");
    let res = call_tool(&api, &t, "upload_file", args(17)).await;
    assert_eq!(res["isError"], true);
    assert_eq!(res["structuredContent"]["status"], 413);
    assert_eq!(
        res["structuredContent"]["error"],
        "file is larger than the MCP upload limit of 16 bytes; upload it with `starstar files upload` instead"
    );
    assert_eq!(puts(&api).len(), 1);
}

#[tokio::test]
async fn upload_cap_boundary_at_one_mib_with_wrapped_base64() {
    let api = TestApi::start_with(Config {
        mcp_max_upload_bytes: 1024 * 1024,
        ..Default::default()
    })
    .await;
    let t = token(&api, Role::Operator);
    let args = |n: usize| {
        json!({"printer": "mk4", "storage": "usb", "path": "a.gcode",
               "content_base64": wrap(&b64(&vec![b'x'; n]), 76)})
    };
    let res = call_tool(&api, &t, "upload_file", args(1024 * 1024)).await;
    assert_eq!(res["isError"], false, "{res}");
    assert_eq!(puts(&api)[0].body.len(), 1024 * 1024);
    let res = call_tool(&api, &t, "upload_file", args(1024 * 1024 + 1)).await;
    assert_eq!(res["structuredContent"]["status"], 413);
    assert_eq!(puts(&api).len(), 1);
}

#[tokio::test]
async fn upload_accepts_wrapped_base64() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Operator);
    let content = b"G28\nG1 X10 Y10 F3000\nM104 S0\n";
    let spaced: String = b64(content)
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
        + "\r\n";
    let res = call_tool(
        &api,
        &t,
        "upload_file",
        json!({"printer": "mk4", "storage": "usb", "path": "a.gcode", "content_base64": spaced}),
    )
    .await;
    assert_eq!(res["isError"], false, "{res}");
    assert_eq!(puts(&api)[0].body, content);
}

#[tokio::test]
async fn upload_rejects_invalid_base64() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Operator);
    let res = call_tool(
        &api,
        &t,
        "upload_file",
        json!({"printer": "mk4", "storage": "usb", "path": "a.gcode", "content_base64": "not base64!"}),
    )
    .await;
    assert_eq!(res["structuredContent"]["status"], 400);
    assert!(puts(&api).is_empty());
}

#[tokio::test]
async fn upload_checks_the_role_before_decoding() {
    let api = TestApi::start_with(Config {
        mcp_max_upload_bytes: 16,
        ..Default::default()
    })
    .await;
    let t = token(&api, Role::Viewer);
    let res = call_tool(
        &api,
        &t,
        "upload_file",
        json!({"printer": "mk4", "storage": "usb", "path": "a.gcode", "content_base64": b64(&[0u8; 100])}),
    )
    .await;
    assert_eq!(res["structuredContent"]["status"], 403);
    assert!(api.fake.requests().is_empty());
}

#[tokio::test]
async fn upload_description_names_the_configured_limit() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Operator);
    let r = post_mcp(&api, Some(&t), HOST, rpc_body("tools/list", json!({}))).await;
    let msg = rpc_result(&r);
    let upload = msg["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "upload_file")
        .unwrap();
    assert_eq!(
        upload["description"],
        "Uploads a file to printer storage. content_base64 is the whole file, base64-encoded; files over the server's MCP upload limit (25 MB) must go through the starstar CLI instead."
    );
}

#[tokio::test]
async fn start_print_and_delete_file() {
    let api = TestApi::start().await;
    let op = token(&api, Role::Operator);
    let res = call_tool(
        &api,
        &op,
        "start_print",
        json!({"printer": "mk4", "storage": "usb", "path": "cube-12mm.bgcode"}),
    )
    .await;
    assert_eq!(text(&res), "Started printing cube-12mm.bgcode on mk4.");
    assert!(fake_saw(
        &api,
        Method::POST,
        "/api/v1/files/usb/cube-12mm.bgcode"
    ));

    let admin = token(&api, Role::Admin);
    let res = call_tool(
        &api,
        &admin,
        "delete_file",
        json!({"printer": "mk4", "storage": "usb", "path": "cube-12mm.bgcode", "force": true}),
    )
    .await;
    assert_eq!(text(&res), "Deleted cube-12mm.bgcode from mk4:usb.");
    let del = api.fake.authorized(Method::DELETE);
    assert_eq!(del.last().unwrap().headers["force"], "?1");
}

#[tokio::test]
async fn camera_snapshot_returns_image_content() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    let res = call_tool(&api, &t, "camera_snapshot", json!({"printer": "mk4"})).await;
    assert_eq!(res["isError"], false, "{res}");
    assert_eq!(res["content"][0]["type"], "image");
    assert_eq!(res["content"][0]["mimeType"], "image/png");
    assert_eq!(
        STANDARD
            .decode(res["content"][0]["data"].as_str().unwrap())
            .unwrap(),
        b"\x89PNG\r\n\x1a\nfake"
    );
    assert!(res.get("structuredContent").is_none());
    call_tool(
        &api,
        &t,
        "camera_snapshot",
        json!({"printer": "mk4", "id": "cam1"}),
    )
    .await;
    assert!(fake_saw(&api, Method::GET, "/api/v1/cameras/cam1/snap"));
    api.fake
        .set(Method::GET, "/api/v1/cameras/snap", Canned::empty(204));
    let res = call_tool(&api, &t, "camera_snapshot", json!({"printer": "mk4"})).await;
    assert_eq!(text(&res), "No picture yet.");
    assert_eq!(res["isError"], false);
}

#[tokio::test]
async fn take_snapshot_posts_and_returns_image() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Operator);
    let res = call_tool(
        &api,
        &t,
        "take_snapshot",
        json!({"printer": "mk4", "id": "cam1"}),
    )
    .await;
    assert_eq!(res["content"][0]["type"], "image");
    assert!(fake_saw(&api, Method::POST, "/api/v1/cameras/cam1/snap"));
}

#[tokio::test]
async fn camera_admin_tools_forward() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Admin);
    let p = |extra: serde_json::Value| {
        let mut v = json!({"printer": "mk4"});
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        v
    };
    let res = call_tool(&api, &t, "list_cameras", p(json!({}))).await;
    assert_eq!(res["structuredContent"]["cameras"][0]["id"], "cam1");
    let res = call_tool(&api, &t, "get_camera", p(json!({"id": "cam1"}))).await;
    assert_eq!(res["structuredContent"]["camera"]["name"], "Buddy3D");
    let res = call_tool(&api, &t, "order_cameras", p(json!({"ids": ["cam1"]}))).await;
    assert_eq!(text(&res), "Reordered cameras on mk4.");
    assert!(fake_saw(&api, Method::PUT, "/api/v1/cameras"));
    let res = call_tool(
        &api,
        &t,
        "register_camera",
        p(json!({"id": "cam1", "name": "Bed"})),
    )
    .await;
    assert_eq!(text(&res), "Registered camera cam1 on mk4.");
    let post = api
        .fake
        .authorized(Method::POST)
        .into_iter()
        .find(|r| r.uri == "/api/v1/cameras/cam1")
        .unwrap();
    assert!(String::from_utf8_lossy(&post.body).contains("Bed"));
    let res = call_tool(
        &api,
        &t,
        "configure_camera",
        p(json!({"id": "cam1", "trigger_scheme": "each_layer", "resolution": {"width": 1280, "height": 720}})),
    )
    .await;
    assert_eq!(text(&res), "Configured camera cam1 on mk4.");
    assert!(fake_saw(&api, Method::PATCH, "/api/v1/cameras/cam1/config"));
    for (tool, verb) in [
        ("reset_camera_config", "Reset the configuration of"),
        ("connect_camera", "Connected"),
        ("disconnect_camera", "Disconnected"),
        ("remove_camera", "Removed"),
    ] {
        let res = call_tool(&api, &t, tool, p(json!({"id": "cam1"}))).await;
        assert_eq!(res["isError"], false, "{tool}: {res}");
        assert_eq!(text(&res), format!("{verb} camera cam1 on mk4."));
    }
    assert!(fake_saw(
        &api,
        Method::DELETE,
        "/api/v1/cameras/cam1/config"
    ));
    assert!(fake_saw(
        &api,
        Method::POST,
        "/api/v1/cameras/cam1/connection"
    ));
    assert!(fake_saw(
        &api,
        Method::DELETE,
        "/api/v1/cameras/cam1/connection"
    ));
    assert!(fake_saw(&api, Method::DELETE, "/api/v1/cameras/cam1"));
    let res = call_tool(&api, &t, "get_camera", p(json!({"id": "../x"}))).await;
    assert_eq!(res["structuredContent"]["status"], 400);
}

#[tokio::test]
async fn update_tools() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Admin);
    let res = call_tool(&api, &t, "check_update", json!({"printer": "mk4"})).await;
    assert_eq!(res["structuredContent"]["update"]["env"], "prusalink");
    assert_eq!(res["structuredContent"]["update"]["available"], true);
    let res = call_tool(&api, &t, "apply_update", json!({"printer": "mk4"})).await;
    assert_eq!(text(&res), "Started the prusalink update on mk4.");
    assert!(fake_saw(&api, Method::POST, "/api/v1/update/prusalink"));
    let res = call_tool(
        &api,
        &t,
        "check_update",
        json!({"printer": "mk4", "env": "other"}),
    )
    .await;
    assert_eq!(res["structuredContent"]["status"], 400);
}

#[tokio::test]
async fn register_printer_never_echoes_password() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Admin);
    let host = api.fake.addr.to_string();
    let res = call_tool(
        &api,
        &t,
        "register_printer",
        json!({"name": "mini", "kind": "mini", "host": host,
               "username": fake_prusalink::USERNAME, "password": PASSWORD}),
    )
    .await;
    assert_eq!(res["isError"], false, "{res}");
    assert_eq!(res["structuredContent"]["printer"]["name"], "mini");
    assert!(!res.to_string().contains(PASSWORD));

    let wrong = "wrong-secret-pw";
    let res = call_tool(
        &api,
        &t,
        "register_printer",
        json!({"name": "mini2", "kind": "mini", "host": host,
               "username": fake_prusalink::USERNAME, "password": wrong}),
    )
    .await;
    assert_eq!(res["isError"], true);
    assert_eq!(res["structuredContent"]["status"], 502);
    assert!(!res.to_string().contains(wrong));

    let res = call_tool(&api, &t, "remove_printer", json!({"printer": "mini"})).await;
    assert_eq!(text(&res), "Removed printer mini.");
    let res = call_tool(&api, &t, "get_printer", json!({"printer": "mini"})).await;
    assert_eq!(res["structuredContent"]["status"], 404);
}
