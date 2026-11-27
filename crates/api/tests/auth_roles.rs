//! Default deny, and the role each auth-owned route needs.

use std::path::Path;

use axum::body::Body;
use axum::http::Request;
use starstar_shared::Role;

mod common;
use common::*;

/// (method, path, minimum role) for every auth-owned route under /api.
const AUTH_ROUTES: &[(&str, &str, Role)] = &[
    ("GET", "/api/me", Role::Viewer),
    ("PUT", "/api/me/password", Role::Viewer),
    ("GET", "/api/users", Role::Admin),
    ("POST", "/api/users", Role::Admin),
    ("PATCH", "/api/users/someone", Role::Admin),
    ("DELETE", "/api/users/someone", Role::Admin),
    ("GET", "/api/apps", Role::Viewer),
    ("GET", "/api/apps?all=true", Role::Admin),
    ("DELETE", "/api/apps/ss_gr_x", Role::Viewer),
];

/// Every /api and /mcp path served today, plus paths no route matches.
const GUARDED: &[(&str, &str)] = &[
    ("GET", "/api/library"),
    ("POST", "/api/library"),
    ("GET", "/api/library/1"),
    ("GET", "/api/library/1/preview"),
    ("GET", "/api/library/1/download"),
    ("DELETE", "/api/library/1"),
    ("GET", "/api/printers"),
    ("POST", "/api/printers"),
    ("GET", "/api/printers/mk4"),
    ("DELETE", "/api/printers/mk4"),
    ("GET", "/api/printers/mk4/info"),
    ("GET", "/api/printers/mk4/status"),
    ("GET", "/api/printers/mk4/storage"),
    ("GET", "/api/printers/mk4/job"),
    ("DELETE", "/api/printers/mk4/job/42"),
    ("PUT", "/api/printers/mk4/job/42/pause"),
    ("PUT", "/api/printers/mk4/job/42/resume"),
    ("PUT", "/api/printers/mk4/job/42/continue"),
    ("GET", "/api/printers/mk4/transfer"),
    ("DELETE", "/api/printers/mk4/transfer/7"),
    ("GET", "/api/printers/mk4/files/usb"),
    ("GET", "/api/printers/mk4/files/usb/cube-12mm.bgcode"),
    ("HEAD", "/api/printers/mk4/files/usb/cube-12mm.bgcode"),
    ("PUT", "/api/printers/mk4/files/usb/cube-12mm.bgcode"),
    ("DELETE", "/api/printers/mk4/files/usb/cube-12mm.bgcode"),
    ("POST", "/api/printers/mk4/files/usb/cube-12mm.bgcode/print"),
    ("GET", "/api/printers/mk4/thumbnails/usb/cube-12mm.bgcode"),
    ("GET", "/api/printers/mk4/cameras"),
    ("GET", "/api/printers/mk4/cameras/snap"),
    ("GET", "/api/printers/mk4/cameras/cam1"),
    ("GET", "/api/printers/mk4/cameras/cam1/snap"),
    ("POST", "/api/printers/mk4/cameras/cam1/snap"),
    ("PUT", "/api/printers/mk4/cameras/order"),
    ("POST", "/api/printers/mk4/cameras/cam1"),
    ("DELETE", "/api/printers/mk4/cameras/cam1"),
    ("PATCH", "/api/printers/mk4/cameras/cam1/config"),
    ("DELETE", "/api/printers/mk4/cameras/cam1/config"),
    ("POST", "/api/printers/mk4/cameras/cam1/connection"),
    ("DELETE", "/api/printers/mk4/cameras/cam1/connection"),
    ("GET", "/api/printers/mk4/update/prusalink"),
    ("POST", "/api/printers/mk4/update/prusalink"),
    ("GET", "/api/session"),
    ("DELETE", "/api/session"),
    ("GET", "/api/does-not-exist"),
    ("POST", "/mcp"),
    ("GET", "/mcp"),
];

fn one_below(role: Role) -> Option<Role> {
    match role {
        Role::Admin => Some(Role::Operator),
        Role::Operator => Some(Role::Viewer),
        Role::Viewer => None,
    }
}

fn req(method: &str, path: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn every_guarded_route_needs_credentials() {
    let (app, _) = test_app().await;
    for (m, p) in GUARDED
        .iter()
        .copied()
        .chain(AUTH_ROUTES.iter().map(|(m, p, _)| (*m, *p)))
    {
        let r = send(&app, req(m, p)).await;
        assert_eq!(r.status, 401, "{m} {p}");
        assert!(r.headers.contains_key("www-authenticate"), "{m} {p}");
    }
}

#[tokio::test]
async fn aliased_paths_are_guarded_too() {
    let (app, _) = test_app().await;
    for path in [
        "//api/printers",
        "///api//printers",
        "/%61pi/printers",
        "/%61%70%69/printers",
        "/api/%70rinters",
        "/api/./printers",
        "/x/../api/printers",
        "/api/%2e/printers",
        "//mcp",
        "/%6dcp/anything",
        "//api/health",
    ] {
        let r = send(&app, req("GET", path)).await;
        assert_eq!(r.status, 401, "{path}");
        assert!(r.headers.contains_key("www-authenticate"), "{path}");
    }
    // Aliases never widen the allowlist: this is not the public health route.
    let r = send(&app, req("POST", "//api/session")).await;
    assert_eq!(r.status, 401, "//api/session");
    // Unrelated paths stay open.
    let r = send(&app, req("GET", "/.well-known/oauth-authorization-server")).await;
    assert_eq!(r.status, 200);
    let r = send(&app, req("GET", "/apiary")).await;
    assert_ne!(r.status, 401);
}

fn collect_rs(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every `.route("…"` path in the source tree.
fn registered_routes() -> Vec<String> {
    let mut files = Vec::new();
    collect_rs(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let mut routes = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(file).unwrap();
        for piece in text.split(".route(").skip(1) {
            let piece = piece.trim_start();
            if let Some(rest) = piece.strip_prefix('"') {
                routes.push(rest.split('"').next().unwrap().to_owned());
            }
        }
    }
    routes
}

#[tokio::test]
async fn every_registered_route_is_denied_without_credentials() {
    let (app, _) = test_app().await;
    let routes = registered_routes();
    assert!(routes.len() >= 5, "route scan found too little: {routes:?}");
    let mut checked = 0;
    for route in routes {
        let exempt = route.starts_with("/oauth/")
            || route.starts_with("/.well-known/")
            || matches!(route.as_str(), "/api/health" | "/api/setup")
            || route == "/api/session";
        // `/api/session` is allowlisted for POST only.
        let guarded = route.starts_with("/api") || route.starts_with("/mcp");
        if !guarded || (exempt && route != "/api/session") {
            continue;
        }
        let path = route
            .replace("{name}", "x")
            .replace("{username}", "x")
            .replace("{id}", "x");
        let methods: &[&str] = if route == "/api/session" {
            &["GET", "DELETE"]
        } else {
            &["GET", "POST", "PUT", "PATCH", "DELETE"]
        };
        for m in methods {
            let r = send(&app, req(m, &path)).await;
            assert_eq!(r.status, 401, "{m} {path}");
        }
        checked += 1;
    }
    assert!(checked >= 5, "only {checked} routes checked");
}

#[tokio::test]
async fn allowlisted_routes_do_not_demand_credentials() {
    let (app, _) = test_app().await;
    assert_eq!(send(&app, req("GET", "/api/health")).await.status, 200);
    // POST /api/session and /api/setup reach their handlers: whatever they
    // answer, it is not require_auth's "sign in required".
    for (uri, body) in [
        (
            "/api/session",
            serde_json::json!({"username": "nobody", "password": "whatever-123"}),
        ),
        (
            "/api/setup",
            serde_json::json!({"setup_code": "x", "username": "a", "password": "pw-12345678"}),
        ),
    ] {
        let r = post_json(&app, uri, body).await;
        assert!(!r.body.contains("sign in required"), "{uri}: {}", r.body);
    }
}

#[tokio::test]
async fn auth_routes_enforce_minimum_role() {
    let (app, state) = test_app().await;
    for (m, p, min) in AUTH_ROUTES {
        if let Some(lower) = one_below(*min) {
            let req = authorize(Request::builder().method(*m).uri(*p), &state, lower)
                .await
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap();
            assert_eq!(send(&app, req).await.status, 403, "{m} {p} as {lower}");
        }
        let req = authorize(Request::builder().method(*m).uri(*p), &state, *min)
            .await
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let status = send(&app, req).await.status;
        assert!(
            status != 401 && status != 403,
            "{m} {p} as {min} gave {status}"
        );
    }
}
