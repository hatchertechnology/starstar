//! JSON-RPC helpers for driving `/mcp` through the in-process router.
#![allow(dead_code)]

use axum::body::Body;
use axum::http::Method;
use serde_json::{Value, json};
use starstar_shared::Role;

use crate::common;
use crate::fake_prusalink::{TestApi, TestResponse};

pub const HOST: &str = "127.0.0.1:8080";

/// A bearer token for a fresh user with `role`.
pub fn token(api: &TestApi, role: Role) -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    common::token_for(&api.state, &format!("mcp-{role}-{n}"), role)
}

pub fn rpc_body(method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
}

/// POSTs one JSON-RPC message to `/mcp`.
pub async fn post_mcp(api: &TestApi, token: Option<&str>, host: &str, body: Value) -> TestResponse {
    let mut headers = vec![
        ("content-type", "application/json".to_owned()),
        ("accept", "application/json, text/event-stream".to_owned()),
        ("mcp-protocol-version", "2025-11-25".to_owned()),
        ("host", host.to_owned()),
    ];
    if let Some(t) = token {
        headers.push(("authorization", format!("Bearer {t}")));
    }
    let refs: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
    api.call_anonymous(Method::POST, "/mcp", &refs, Body::from(body.to_string()))
        .await
}

/// The JSON-RPC message in a response: plain JSON, or the last SSE `data:` line.
pub fn rpc_result(r: &TestResponse) -> Value {
    let text = r.text();
    let ct = r
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if ct.starts_with("text/event-stream") {
        let data = text
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim)
            .rfind(|d| !d.is_empty())
            .unwrap_or_else(|| panic!("no data line in {text}"));
        serde_json::from_str(data).unwrap()
    } else {
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"))
    }
}

pub async fn list_tools(api: &TestApi, token: &str) -> Vec<String> {
    let r = post_mcp(api, Some(token), HOST, rpc_body("tools/list", json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let msg = rpc_result(&r);
    msg["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("no tools in {msg}"))
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect()
}

/// The whole JSON-RPC message for a `tools/call`.
pub async fn call_tool_raw(api: &TestApi, token: &str, name: &str, args: Value) -> Value {
    let r = post_mcp(
        api,
        Some(token),
        HOST,
        rpc_body("tools/call", json!({"name": name, "arguments": args})),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    rpc_result(&r)
}

/// The `result` of a `tools/call`; panics on a JSON-RPC error.
pub async fn call_tool(api: &TestApi, token: &str, name: &str, args: Value) -> Value {
    let msg = call_tool_raw(api, token, name, args).await;
    assert!(msg.get("error").is_none(), "JSON-RPC error: {msg}");
    msg["result"].clone()
}
