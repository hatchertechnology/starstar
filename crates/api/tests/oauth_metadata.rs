//! OAuth metadata documents and dynamic client registration.

use axum::body::Body;
use axum::http::Request;
use serde_json::json;

mod common;
use common::*;

async fn get(app: &axum::Router, path: &str) -> TestResponse {
    send(app, Request::get(path).body(Body::empty()).unwrap()).await
}

#[tokio::test]
async fn authorization_server_metadata() {
    let (app, _) = test_app();
    let r = get(&app, "/.well-known/oauth-authorization-server").await;
    assert_eq!(r.status, 200);
    let j = r.json();
    assert_eq!(j["issuer"], "http://127.0.0.1:8080");
    assert_eq!(j["token_endpoint"], "http://127.0.0.1:8080/oauth/token");
    assert_eq!(
        j["registration_endpoint"],
        "http://127.0.0.1:8080/oauth/register"
    );
    assert_eq!(j["code_challenge_methods_supported"], json!(["S256"]));
    assert_eq!(j["token_endpoint_auth_methods_supported"], json!(["none"]));
    assert_eq!(r.headers["cache-control"], "max-age=300");
}

#[tokio::test]
async fn protected_resource_metadata_root_and_mcp() {
    let (app, _) = test_app();
    let r = get(&app, "/.well-known/oauth-protected-resource").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["resource"], "http://127.0.0.1:8080");
    assert_eq!(
        r.json()["authorization_servers"],
        json!(["http://127.0.0.1:8080"])
    );
    let r = get(&app, "/.well-known/oauth-protected-resource/mcp").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["resource"], "http://127.0.0.1:8080/mcp");
    // The document the /mcp challenge points at really exists.
    let challenge = get(&app, "/mcp").await;
    assert!(
        challenge.headers["www-authenticate"]
            .to_str()
            .unwrap()
            .contains("/.well-known/oauth-protected-resource/mcp")
    );
}

#[tokio::test]
async fn registers_public_client() {
    let (app, state) = test_app();
    let r = post_json(
        &app,
        "/oauth/register",
        json!({"client_name":"Claude Code","redirect_uris":["http://localhost:33418/callback"]}),
    )
    .await;
    assert_eq!(r.status, 201);
    let id = r.json()["client_id"].as_str().unwrap().to_owned();
    assert!(id.starts_with("ss_cl_"));
    assert_eq!(r.json()["token_endpoint_auth_method"], "none");
    assert_eq!(state.auth.client(&id).unwrap().unwrap().name, "Claude Code");
}

#[tokio::test]
async fn registration_rejections() {
    let (app, _) = test_app();
    let reg = |v: serde_json::Value| post_json(&app, "/oauth/register", v);
    let cases = [
        (json!({"redirect_uris": []}), "invalid_redirect_uri"),
        (
            json!({"redirect_uris": ["http://192.168.1.9/cb"]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris": ["http://127.0.0.1:8080/oauth/code"]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris": ["http://127.0.0.1:9/cb"], "token_endpoint_auth_method": "client_secret_basic"}),
            "invalid_client_metadata",
        ),
        (
            json!({"redirect_uris": ["http://127.0.0.1:9/cb"], "grant_types": ["client_credentials"]}),
            "invalid_client_metadata",
        ),
        (
            json!({"redirect_uris": (0..11).map(|i| format!("https://a.example/{i}")).collect::<Vec<_>>()}),
            "invalid_redirect_uri",
        ),
    ];
    for (body, error) in cases {
        let r = reg(body.clone()).await;
        assert_eq!(r.status, 400, "{body}");
        assert_eq!(r.json()["error"], error, "{body}");
    }
}

#[tokio::test]
async fn client_name_is_sanitized() {
    let (app, state) = test_app();
    let name = |n: Option<String>| {
        let mut body = json!({"redirect_uris":["https://a.example/cb"]});
        if let Some(n) = n {
            body["client_name"] = json!(n);
        }
        let (app, state) = (app.clone(), state.clone());
        async move {
            let r = post_json(&app, "/oauth/register", body).await;
            assert_eq!(r.status, 201);
            state
                .auth
                .client(r.json()["client_id"].as_str().unwrap())
                .unwrap()
                .unwrap()
                .name
        }
    };
    assert_eq!(name(Some("  Evil\u{0007}App  ".into())).await, "EvilApp");
    assert_eq!(name(None).await, "Unnamed app");
    assert_eq!(name(Some("x".repeat(150))).await.chars().count(), 100);
}

#[tokio::test]
async fn registration_is_rate_limited_per_ip() {
    let (app, state) = test_app();
    let body = json!({"redirect_uris":["https://a.example/cb"]});
    for _ in 0..20 {
        assert_eq!(
            post_json(&app, "/oauth/register", body.clone())
                .await
                .status,
            201
        );
    }
    assert_eq!(
        post_json(&app, "/oauth/register", body.clone())
            .await
            .status,
        429
    );
    state.auth.clock().advance(3601);
    assert_eq!(post_json(&app, "/oauth/register", body).await.status, 201);
}
