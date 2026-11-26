mod common;
mod fake_prusalink;
mod mcp_support;

use axum::body::Body;
use axum::http::Method;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as STANDARD_B64;
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

fn names(list: &[&str]) -> std::collections::BTreeSet<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

const VIEWER_TOOLS: &[&str] = &[
    "list_printers",
    "get_printer",
    "printer_info",
    "printer_status",
    "list_storage",
    "get_job",
    "get_transfer",
    "list_files",
    "file_state",
    "list_cameras",
    "get_camera",
    "camera_snapshot",
];
const OPERATOR_TOOLS: &[&str] = &[
    "pause_job",
    "resume_job",
    "continue_job",
    "stop_job",
    "cancel_transfer",
    "upload_file",
    "start_print",
    "take_snapshot",
];
const ADMIN_TOOLS: &[&str] = &[
    "register_printer",
    "remove_printer",
    "delete_file",
    "order_cameras",
    "register_camera",
    "remove_camera",
    "configure_camera",
    "reset_camera_config",
    "connect_camera",
    "disconnect_camera",
    "check_update",
    "apply_update",
];

#[tokio::test]
async fn tools_list_per_role() {
    let api = TestApi::start().await;
    let listed = |role| {
        let api = &api;
        async move {
            let t = token(api, role);
            list_tools(api, &t)
                .await
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
        }
    };
    let viewer = names(VIEWER_TOOLS);
    let operator: Vec<&str> = [VIEWER_TOOLS, OPERATOR_TOOLS].concat();
    let admin: Vec<&str> = [VIEWER_TOOLS, OPERATOR_TOOLS, ADMIN_TOOLS].concat();
    assert_eq!(listed(Role::Viewer).await, viewer);
    assert_eq!(listed(Role::Operator).await, names(&operator));
    let all = listed(Role::Admin).await;
    assert_eq!(all, names(&admin));
    assert_eq!(all.len(), 32);

    let t = token(&api, Role::Operator);
    let r = post_mcp(&api, Some(&t), HOST, rpc_body("tools/list", json!({}))).await;
    let msg = rpc_result(&r);
    let stop = msg["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "stop_job")
        .unwrap();
    assert_eq!(stop["annotations"]["readOnlyHint"], false);
    assert_eq!(stop["annotations"]["destructiveHint"], true);
}

#[tokio::test]
async fn every_tool_rejects_the_role_below_it() {
    let api = TestApi::start().await;
    let up = STANDARD_B64.encode(b"abc");
    let cases: Vec<(&str, serde_json::Value, Role)> = {
        let p = |extra: serde_json::Value| {
            let mut v = json!({"printer": "mk4"});
            v.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            v
        };
        let f = json!({"storage": "usb", "path": "a.bgcode"});
        let cam = json!({"id": "cam1"});
        vec![
            ("pause_job", p(json!({"id": 42})), Role::Viewer),
            ("resume_job", p(json!({"id": 42})), Role::Viewer),
            ("continue_job", p(json!({"id": 42})), Role::Viewer),
            ("stop_job", p(json!({"id": 42})), Role::Viewer),
            ("cancel_transfer", p(json!({"id": 7})), Role::Viewer),
            (
                "upload_file",
                p(json!({"storage": "usb", "path": "a.bgcode", "content_base64": up})),
                Role::Viewer,
            ),
            ("start_print", p(f.clone()), Role::Viewer),
            ("take_snapshot", p(cam.clone()), Role::Viewer),
            (
                "register_printer",
                json!({"name": "x", "kind": "mk4", "host": "127.0.0.1:1", "username": "u", "password": "p"}),
                Role::Operator,
            ),
            ("remove_printer", p(json!({})), Role::Operator),
            ("delete_file", p(f), Role::Operator),
            ("order_cameras", p(json!({"ids": ["cam1"]})), Role::Operator),
            ("register_camera", p(cam.clone()), Role::Operator),
            ("remove_camera", p(cam.clone()), Role::Operator),
            ("configure_camera", p(cam.clone()), Role::Operator),
            ("reset_camera_config", p(cam.clone()), Role::Operator),
            ("connect_camera", p(cam.clone()), Role::Operator),
            ("disconnect_camera", p(cam), Role::Operator),
            ("check_update", p(json!({})), Role::Operator),
            ("apply_update", p(json!({})), Role::Operator),
        ]
    };
    assert_eq!(cases.len(), OPERATOR_TOOLS.len() + ADMIN_TOOLS.len());
    for (tool, args, lower) in cases {
        let t = token(&api, lower);
        let res = call_tool(&api, &t, tool, args).await;
        assert_eq!(res["isError"], true, "{tool}: {res}");
        assert_eq!(res["structuredContent"]["status"], 403, "{tool}: {res}");
    }
    assert!(api.fake.requests().is_empty());
}

#[tokio::test]
async fn hidden_tool_called_by_name_is_forbidden_not_unknown() {
    let api = TestApi::start().await;
    let t = token(&api, Role::Viewer);
    let msg = call_tool_raw(&api, &t, "apply_update", json!({"printer": "mk4"})).await;
    assert!(msg.get("error").is_none(), "{msg}");
    assert_eq!(msg["result"]["isError"], true);
    assert_eq!(msg["result"]["structuredContent"]["status"], 403);
}

#[tokio::test]
async fn demoted_caller_loses_tools_on_next_request() {
    let api = TestApi::start().await;
    let t = common::token_for(&api.state, "op", Role::Operator);
    assert!(list_tools(&api, &t).await.contains(&"pause_job".to_owned()));
    let res = call_tool(&api, &t, "pause_job", json!({"printer": "mk4", "id": 42})).await;
    assert_eq!(res["isError"], false, "{res}");

    api.state
        .auth
        .update_user("op", Some(Role::Viewer), None)
        .unwrap();
    assert!(!list_tools(&api, &t).await.contains(&"pause_job".to_owned()));
    let res = call_tool(&api, &t, "pause_job", json!({"printer": "mk4", "id": 42})).await;
    assert_eq!(res["structuredContent"]["status"], 403);
}

#[tokio::test]
async fn disabled_user_gets_401_on_next_mcp_request() {
    let api = TestApi::start().await;
    common::create_user(&api.state, "boss", "pw", Role::Admin);
    let t = common::token_for(&api.state, "v", Role::Viewer);
    let res = call_tool(&api, &t, "printer_status", json!({"printer": "mk4"})).await;
    assert_eq!(res["isError"], false);
    api.state.auth.update_user("v", None, Some(true)).unwrap();
    let r = post_mcp(&api, Some(&t), HOST, rpc_body("tools/list", json!({}))).await;
    assert_eq!(r.status, 401);
    assert_eq!(
        r.headers["www-authenticate"],
        r#"Bearer error="invalid_token", resource_metadata="http://127.0.0.1:8080/.well-known/oauth-protected-resource/mcp""#
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn end_to_end_initialize_and_tools_call_over_tcp() {
    let api = TestApi::start().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = api.app.clone();
    tokio::spawn(async move { axum::serve(listener, app).await });
    let http = reqwest::Client::new();

    // 1. No token: 401 with the /mcp challenge, and the metadata it points to exists.
    let r = http
        .post(format!("{base}/mcp"))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(rpc_body("tools/list", json!({})).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    let challenge = r.headers()["www-authenticate"].to_str().unwrap().to_owned();
    assert!(
        challenge.contains("/.well-known/oauth-protected-resource/mcp\""),
        "{challenge}"
    );
    let r = http
        .get(format!("{base}/.well-known/oauth-protected-resource/mcp"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let doc: serde_json::Value = r.json().await.unwrap();
    assert_eq!(doc["resource"], "http://127.0.0.1:8080/mcp");

    // 2-7. A signed-in operator talks MCP.
    let token = common::token_for(&api.state, "agent", Role::Operator);
    let send = |body: serde_json::Value, version: bool| {
        let mut req = http
            .post(format!("{base}/mcp"))
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream");
        if version {
            req = req.header("mcp-protocol-version", "2025-11-25");
        }
        req.body(body.to_string()).send()
    };
    let r = send(
        rpc_body(
            "initialize",
            json!({"protocolVersion": "2025-11-25", "capabilities": {},
                   "clientInfo": {"name": "e2e", "version": "1"}}),
        ),
        false,
    )
    .await
    .unwrap();
    assert_eq!(r.status(), 200);
    assert!(
        r.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("application/json")
    );
    let msg: serde_json::Value = r.json().await.unwrap();
    assert_eq!(msg["result"]["serverInfo"]["name"], "starstar");

    let r = send(
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        true,
    )
    .await
    .unwrap();
    assert_eq!(r.status(), 202);

    let r = send(rpc_body("tools/list", json!({})), true).await.unwrap();
    let msg: serde_json::Value = r.json().await.unwrap();
    let names: Vec<&str> = msg["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"pause_job"));
    assert!(!names.contains(&"apply_update"));

    let call = |name: &str, args: serde_json::Value| {
        send(
            rpc_body("tools/call", json!({"name": name, "arguments": args})),
            true,
        )
    };
    let r = call("printer_status", json!({"printer": "mk4"}))
        .await
        .unwrap();
    let msg: serde_json::Value = r.json().await.unwrap();
    assert_eq!(msg["result"]["structuredContent"]["status"]["name"], "mk4");

    let r = call("pause_job", json!({"printer": "mk4"})).await.unwrap();
    let msg: serde_json::Value = r.json().await.unwrap();
    assert_eq!(msg["result"]["content"][0]["text"], "Paused job 42 on mk4.");
    assert!(
        api.fake
            .authorized(Method::PUT)
            .iter()
            .any(|r| r.uri == "/api/v1/job/42/pause")
    );
}

#[tokio::test]
async fn body_over_the_limit_is_413() {
    // 1000 bytes -> 1336 base64 chars; limit = 1336 + 83 + 64 KiB.
    let limit = 1336 + 1336 / 16 + 64 * 1024;
    let api = TestApi::start_with(Config {
        mcp_max_upload_bytes: 1000,
        ..Config::default()
    })
    .await;
    let t = token(&api, Role::Viewer);
    let padded = |len: usize| {
        let base = rpc_body("tools/list", json!({"pad": ""})).to_string().len();
        rpc_body("tools/list", json!({"pad": "a".repeat(len - base)}))
    };
    let ok = post_mcp(&api, Some(&t), HOST, padded(limit)).await;
    assert_eq!(ok.status, 200, "{}", ok.text());
    let r = post_mcp(&api, Some(&t), HOST, padded(limit + 1)).await;
    assert_eq!(r.status, 413, "{}", r.text());
}
