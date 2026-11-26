//! The web UI fallback, through the full router.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use common::{TestResponse, authorize, send, test_state};
use starstar_api::AppState;
use starstar_api::config::Config;
use starstar_shared::Role;
use tempfile::TempDir;

const MARKER: &str = "SPA-MARKER";

fn web_app() -> (Router, AppState, TempDir) {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("public");
    std::fs::create_dir_all(dir.join("_nuxt/builds/meta")).unwrap();
    std::fs::write(
        dir.join("index.html"),
        format!("<!doctype html><title>starstar</title><div id=\"__nuxt\">{MARKER}</div>"),
    )
    .unwrap();
    std::fs::write(dir.join("_nuxt/app.abc123.js"), "console.log(1)").unwrap();
    std::fs::write(dir.join("_nuxt/builds/latest.json"), "{}").unwrap();
    std::fs::write(dir.join("favicon.ico"), "ico").unwrap();
    std::fs::write(tmp.path().join("secret.txt"), "TOP-SECRET").unwrap();
    let state = state_with(dir.to_str().unwrap());
    (starstar_api::router(state.clone()), state, tmp)
}

fn state_with(web_dir: &str) -> AppState {
    let base = test_state();
    AppState {
        config: Arc::new(Config {
            web_dir: web_dir.to_owned(),
            ..Config::default()
        }),
        ..base
    }
}

async fn get(app: &Router, uri: &str) -> TestResponse {
    send(app, Request::get(uri).body(Body::empty()).unwrap()).await
}

fn header<'a>(r: &'a TestResponse, name: &str) -> &'a str {
    r.headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

#[tokio::test]
async fn serves_index_at_root() {
    let (app, _, _t) = web_app();
    let r = get(&app, "/").await;
    assert_eq!(r.status, 200);
    assert!(header(&r, "content-type").starts_with("text/html"));
    assert!(r.body.contains(MARKER));
    assert_eq!(header(&r, "cache-control"), "no-cache");
}

#[tokio::test]
async fn serves_hashed_asset_with_long_cache() {
    let (app, _, _t) = web_app();
    let r = get(&app, "/_nuxt/app.abc123.js").await;
    assert_eq!(r.status, 200);
    assert!(header(&r, "content-type").contains("javascript"));
    assert!(header(&r, "cache-control").contains("immutable"));
}

#[tokio::test]
async fn not_modified_hashed_asset_stays_immutable() {
    let (app, _, _t) = web_app();
    let first = get(&app, "/_nuxt/app.abc123.js").await;
    let modified = header(&first, "last-modified").to_owned();
    assert!(!modified.is_empty());
    let r = send(
        &app,
        Request::get("/_nuxt/app.abc123.js")
            .header("if-modified-since", modified)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 304);
    assert_eq!(
        header(&r, "cache-control"),
        "public, max-age=31536000, immutable"
    );
    // Build metadata is never immutable, even when revalidated.
    let first = get(&app, "/_nuxt/builds/latest.json").await;
    let r = send(
        &app,
        Request::get("/_nuxt/builds/latest.json")
            .header("if-modified-since", header(&first, "last-modified"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 304);
    assert_eq!(header(&r, "cache-control"), "no-cache");
}

#[tokio::test]
async fn build_metadata_is_not_immutable() {
    let (app, _, _t) = web_app();
    let r = get(&app, "/_nuxt/builds/latest.json").await;
    assert_eq!(r.status, 200);
    assert_eq!(header(&r, "cache-control"), "no-cache");
}

#[tokio::test]
async fn spa_fallback_for_client_routes() {
    let (app, _, _t) = web_app();
    for uri in [
        "/printers/mk4",
        "/consent?request=abc",
        "/login?next=%2Fconsent%3Frequest%3Dabc",
        "/users",
    ] {
        let r = get(&app, uri).await;
        assert_eq!(r.status, 200, "{uri}");
        assert!(r.body.contains(MARKER), "{uri}");
    }
}

#[tokio::test]
async fn head_on_client_route() {
    let (app, _, _t) = web_app();
    let r = send(
        &app,
        Request::head("/printers/mk4").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(r.status, 200);
    assert!(r.body.is_empty());
}

#[tokio::test]
async fn missing_asset_is_404_not_index() {
    let (app, _, _t) = web_app();
    for uri in ["/_nuxt/old.js", "/missing.png"] {
        let r = get(&app, uri).await;
        assert_eq!(r.status, 404, "{uri}");
        assert!(!r.body.contains(MARKER), "{uri}");
    }
}

#[tokio::test]
async fn reserved_prefixes_never_get_index() {
    let (app, state, _t) = web_app();
    let r = send(
        &app,
        authorize(Request::get("/api/does-not-exist"), &state, Role::Admin)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 404);
    assert!(header(&r, "content-type").starts_with("application/json"));
    assert!(r.json()["error"].is_string());
    assert!(!r.body.contains(MARKER));
    for uri in [
        "/oauth/does-not-exist",
        "/.well-known/does-not-exist",
        "/oauth",
    ] {
        let r = get(&app, uri).await;
        assert_eq!(r.status, 404, "{uri}");
        assert!(
            header(&r, "content-type").starts_with("application/json"),
            "{uri}"
        );
        assert_eq!(r.json()["error"], "not found", "{uri}");
        assert!(!r.body.contains(MARKER), "{uri}");
    }
    // Normalization tricks still land on the JSON 404, never the SPA.
    for uri in ["//api/x", "/%61pi/x", "/.well-known//x", "/oauth//zzz"] {
        let r = send(
            &app,
            authorize(Request::get(uri), &state, Role::Admin)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(r.status, 404, "{uri}");
        assert!(
            header(&r, "content-type").starts_with("application/json"),
            "{uri}"
        );
        assert_eq!(r.json()["error"], "not found", "{uri}");
        assert!(!r.body.contains(MARKER), "{uri}");
    }
    let r = send(
        &app,
        authorize(Request::get("/mcp/zzz"), &state, Role::Admin)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 404);
    assert!(header(&r, "content-type").starts_with("application/json"));
    assert_eq!(r.json()["error"], "not found");
    assert!(!r.body.contains(MARKER));
    let r = get(&app, "/.well-known/does-not-exist").await;
    assert_eq!(r.json()["error"], "not found");
}

#[tokio::test]
async fn unauthenticated_unknown_api_path_is_still_401() {
    let (app, _, _t) = web_app();
    let r = get(&app, "/api/does-not-exist").await;
    assert_eq!(r.status, 401);
    assert!(header(&r, "www-authenticate").contains("resource_metadata"));
}

#[tokio::test]
async fn prefix_lookalikes_are_client_routes() {
    let (app, _, _t) = web_app();
    for uri in ["/apiary", "/oauthx", "/mcpx"] {
        let r = get(&app, uri).await;
        assert_eq!(r.status, 200, "{uri}");
        assert!(r.body.contains(MARKER), "{uri}");
    }
}

#[tokio::test]
async fn real_routes_still_win() {
    let (app, state, _t) = web_app();
    let r = send(
        &app,
        authorize(Request::get("/api/health"), &state, Role::Viewer)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(r.json().is_object());
    let r = get(&app, "/.well-known/oauth-authorization-server").await;
    assert_eq!(r.status, 200);
    assert!(r.json().get("issuer").is_some());
}

#[tokio::test]
async fn non_get_on_ui_path_is_405() {
    let (app, _, _t) = web_app();
    let r = send(
        &app,
        Request::post("/printers/mk4").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(r.status, 405);
    assert_eq!(header(&r, "allow"), "GET, HEAD");
}

#[tokio::test]
async fn traversal_does_not_escape() {
    let (app, _, _t) = web_app();
    for uri in [
        "/../secret.txt",
        "/%2e%2e/secret.txt",
        "/_nuxt/%2e%2e/%2e%2e/secret.txt",
        "/..%2fsecret.txt",
    ] {
        let r = get(&app, uri).await;
        assert!(!r.body.contains("TOP-SECRET"), "{uri}");
    }
}

#[tokio::test]
async fn unbuilt_ui_is_503() {
    let tmp = TempDir::new().unwrap();
    let missing = tmp.path().join("nope");
    let state = state_with(missing.to_str().unwrap());
    let app = starstar_api::router(state.clone());
    let r = get(&app, "/").await;
    assert_eq!(r.status, 503);
    assert!(r.body.contains("not built"));
    let r = send(
        &app,
        authorize(Request::get("/api/health"), &state, Role::Viewer)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 200);
}

#[tokio::test]
async fn security_headers_on_static_and_fallback() {
    let (app, _, _t) = web_app();
    for uri in [
        "/",
        "/_nuxt/app.abc123.js",
        "/printers/mk4",
        "/_nuxt/old.js",
    ] {
        let r = get(&app, uri).await;
        assert_eq!(header(&r, "x-frame-options"), "DENY", "{uri}");
        assert!(
            header(&r, "content-security-policy").contains("frame-ancestors 'none'"),
            "{uri}"
        );
        assert_eq!(header(&r, "x-content-type-options"), "nosniff", "{uri}");
    }
}
