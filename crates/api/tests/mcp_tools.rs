mod common;
mod fake_prusalink;
mod mcp_support;

use std::time::Duration;

use axum::http::Method;
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
