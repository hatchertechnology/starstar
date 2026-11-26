mod common;
mod fake_prusalink;
mod mcp_support;

use axum::body::Body;
use axum::http::Method;
use fake_prusalink::TestApi;
use mcp_support::*;
use serde_json::json;
use starstar_api::config::Config;
use starstar_shared::Role;

#[tokio::test]
async fn unauthenticated_mcp_gets_401_with_mcp_resource_metadata() {
    let api = TestApi::start().await;
    let r = post_mcp(&api, None, HOST, rpc_body("tools/list", json!({}))).await;
    assert_eq!(r.status, 401);
    assert_eq!(
        r.headers["www-authenticate"],
        r#"Bearer resource_metadata="http://127.0.0.1:8080/.well-known/oauth-protected-resource/mcp""#
    );
}

#[tokio::test]
async fn initialize_reports_starstar_with_tools() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    let r = post_mcp(
        &api,
        Some(&t),
        HOST,
        rpc_body(
            "initialize",
            json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}),
        ),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    let msg = rpc_result(&r);
    assert_eq!(msg["result"]["serverInfo"]["name"], "starstar");
    assert!(msg["result"]["capabilities"]["tools"].is_object());
    assert!(
        msg["result"]["instructions"]
            .as_str()
            .unwrap()
            .starts_with("Controls the 3D printers registered with starstar.")
    );
}

#[tokio::test]
async fn tools_list_works_without_a_session() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    let r = post_mcp(&api, Some(&t), HOST, rpc_body("tools/list", json!({}))).await;
    assert_eq!(r.status, 200);
    assert!(r.headers.get("mcp-session-id").is_none());
    let names = list_tools(&api, &t).await;
    assert!(names.contains(&"list_printers".to_owned()));
    assert!(names.contains(&"printer_status".to_owned()));
}

#[tokio::test]
async fn printer_status_tool_returns_structured_status() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    let res = call_tool(&api, &t, "printer_status", json!({"printer": "mk4"})).await;
    assert_eq!(res["isError"], false);
    assert_eq!(res["structuredContent"]["status"]["name"], "mk4");
    assert!(
        api.fake
            .authorized(Method::GET)
            .iter()
            .any(|r| r.uri == "/api/v1/status")
    );
}

#[tokio::test]
async fn list_printers_tool_lists_registered_printers() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    let res = call_tool(&api, &t, "list_printers", json!({})).await;
    assert_eq!(res["structuredContent"]["printers"][0]["name"], "mk4");
    assert!(!res.to_string().contains(fake_prusalink::PASSWORD));
}

#[tokio::test]
async fn unknown_printer_is_a_404_tool_error() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    let res = call_tool(&api, &t, "printer_status", json!({"printer": "nope"})).await;
    assert_eq!(res["isError"], true);
    assert_eq!(res["structuredContent"]["status"], 404);
    assert!(
        res["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("not found (404): ")
    );
}

#[tokio::test]
async fn unknown_tool_is_a_protocol_error() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    let msg = call_tool_raw(&api, &t, "no_such_tool", json!({})).await;
    assert_eq!(msg["error"]["code"], -32602, "{msg}");
    assert!(msg.get("result").is_none());
}

#[tokio::test]
async fn host_header_must_match_public_url_or_loopback() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    let body = || rpc_body("tools/list", json!({}));
    assert_eq!(
        post_mcp(&api, Some(&t), "evil.example", body())
            .await
            .status,
        403
    );
    assert_eq!(
        post_mcp(&api, Some(&t), "localhost:9999", body())
            .await
            .status,
        200
    );

    let api = TestApi::start_with(Config {
        public_url: "https://print.example.com".into(),
        ..Default::default()
    })
    .await;
    let t = token(&api, Role::Viewer);
    assert_eq!(
        post_mcp(&api, Some(&t), "print.example.com", body())
            .await
            .status,
        200
    );
    assert_eq!(
        post_mcp(&api, Some(&t), "other.example.com", body())
            .await
            .status,
        403
    );
}

#[tokio::test]
async fn get_and_delete_are_not_allowed_in_stateless_mode() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    let auth = format!("Bearer {t}");
    for method in [Method::GET, Method::DELETE] {
        let r = api
            .call_anonymous(
                method.clone(),
                "/mcp",
                &[
                    ("authorization", &auth),
                    ("host", HOST),
                    ("accept", "text/event-stream"),
                ],
                Body::empty(),
            )
            .await;
        assert_eq!(r.status, 405, "{method}: {}", r.text());
    }
}

#[tokio::test]
async fn missing_host_is_400() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    let auth = format!("Bearer {t}");
    let r = api
        .call_anonymous(
            Method::POST,
            "/mcp",
            &[
                ("authorization", &auth),
                ("content-type", "application/json"),
                ("accept", "application/json, text/event-stream"),
            ],
            Body::from(rpc_body("tools/list", json!({})).to_string()),
        )
        .await;
    assert_eq!(r.status, 400, "{}", r.text());
}

#[tokio::test]
async fn head_and_options_are_405() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    let auth = format!("Bearer {t}");
    for method in [Method::HEAD, Method::OPTIONS] {
        let r = api
            .call_anonymous(
                method.clone(),
                "/mcp",
                &[("authorization", &auth), ("host", HOST)],
                Body::empty(),
            )
            .await;
        assert_eq!(r.status, 405, "{method}");
    }
}

#[tokio::test]
async fn origin_must_match_public_url_or_loopback() {
    let api = TestApi::start_with(Config {
        public_url: "https://print.example.com".into(),
        ..Default::default()
    })
    .await;
    let t = token(&api, Role::Viewer);
    let auth = format!("Bearer {t}");
    let send = |origin: Option<&'static str>| {
        let api = &api;
        let auth = auth.clone();
        async move {
            let mut headers = vec![
                ("authorization", auth.as_str()),
                ("host", "print.example.com"),
                ("content-type", "application/json"),
                ("accept", "application/json, text/event-stream"),
            ];
            if let Some(o) = origin {
                headers.push(("origin", o));
            }
            api.call_anonymous(
                Method::POST,
                "/mcp",
                &headers,
                Body::from(rpc_body("tools/list", json!({})).to_string()),
            )
            .await
            .status
        }
    };
    assert_eq!(send(Some("https://evil.example")).await, 403);
    assert_eq!(send(Some("https://print.example.com")).await, 200);
    assert_eq!(send(Some("http://localhost:3000")).await, 200);
    assert_eq!(send(None).await, 200);
}
