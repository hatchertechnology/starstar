//! Static file serving for the web UI, with an SPA fallback.
//!
//! The router has no routes, only a fallback, and is merged last in
//! [`crate::router`] so it only sees paths nothing else matched. It never
//! returns `index.html` for reserved prefixes (`/api`, `/oauth`, `/mcp`,
//! `/.well-known`) or for a missing asset.

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{ALLOW, CACHE_CONTROL, CONTENT_TYPE, REFERRER_POLICY};
use axum::http::{HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

use crate::AppState;
use crate::auth::normalize_path;
use crate::error::ApiError;

/// Body of the 503 served when the UI has not been built.
pub const UNBUILT_MESSAGE: &str = "starstar web UI is not built. Run `npm --prefix web ci && npm --prefix web run generate`, or set STARSTAR_WEB_DIR.";

const RESERVED: [&str; 4] = ["/api", "/oauth", "/mcp", "/.well-known"];
const ASSET_EXTENSIONS: [&str; 15] = [
    "js",
    "mjs",
    "css",
    "map",
    "png",
    "jpg",
    "jpeg",
    "svg",
    "ico",
    "webp",
    "woff",
    "woff2",
    "json",
    "txt",
    "webmanifest",
];

/// The router: a fallback handler and nothing else.
pub fn router(state: AppState) -> Router {
    Router::new().fallback(serve).with_state(state)
}

/// True for `/api`, `/oauth`, `/mcp` and `/.well-known`, exactly or followed by `/`.
pub fn is_reserved(path: &str) -> bool {
    RESERVED
        .iter()
        .any(|p| path == *p || path.strip_prefix(p).is_some_and(|r| r.starts_with('/')))
}

fn is_asset_path(path: &str) -> bool {
    if path.starts_with("/_nuxt/") {
        return true;
    }
    let last = path.rsplit('/').next().unwrap_or("");
    match last.rsplit_once('.') {
        Some((stem, ext)) => {
            let ext = ext.to_ascii_lowercase();
            (!stem.is_empty() || !ext.is_empty())
                && (ASSET_EXTENSIONS.contains(&ext.as_str()) || ext == "xml")
        }
        None => false,
    }
}

async fn serve(State(state): State<AppState>, req: Request) -> Response {
    let path = req.uri().path().to_owned();
    // Reserved paths are checked on the normalized path, so `//api/x` and
    // `/%61pi/x` cannot reach the SPA.
    if is_reserved(&path) || is_reserved(&normalize_path(&path)) {
        return ApiError::NotFound("not found".to_owned()).into_response();
    }
    let (parts, _body) = req.into_parts();
    let method = parts.method.clone();
    if method != Method::GET && method != Method::HEAD {
        let mut r = StatusCode::METHOD_NOT_ALLOWED.into_response();
        r.headers_mut()
            .insert(ALLOW, HeaderValue::from_static("GET, HEAD"));
        return finish(r, &path);
    }
    let dir = state.config.web_dir.clone();
    let copy = || {
        let mut b = Request::builder()
            .method(method.clone())
            .uri(parts.uri.clone());
        if let Some(h) = b.headers_mut() {
            *h = parts.headers.clone();
        }
        b.body(Body::empty()).expect("request copy")
    };
    let response = match ServeDir::new(&dir).oneshot(copy()).await {
        Ok(r) => r.map(Body::new),
        Err(never) => match never {},
    };
    if response.status() != StatusCode::NOT_FOUND || is_asset_path(&path) {
        return finish(response, &path);
    }
    let index = format!("{dir}/index.html");
    let response = match ServeFile::new(index).oneshot(copy()).await {
        Ok(r) => r.map(Body::new),
        Err(never) => match never {},
    };
    if response.status() == StatusCode::NOT_FOUND {
        let mut r = (StatusCode::SERVICE_UNAVAILABLE, UNBUILT_MESSAGE).into_response();
        r.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        return finish(r, &path);
    }
    finish(response, &path)
}

/// Adds the headers this module owns. Never touches `X-Frame-Options` or
/// `Content-Security-Policy`; the auth layer sets those.
fn finish(mut response: Response, path: &str) -> Response {
    let ok = response.status() == StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("same-origin"));
    let immutable = ok && path.starts_with("/_nuxt/") && !path.starts_with("/_nuxt/builds/");
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static(if immutable {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        }),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_prefixes() {
        for p in [
            "/api",
            "/api/",
            "/api/x",
            "/oauth",
            "/oauth/authorize",
            "/mcp",
            "/mcp/x",
            "/.well-known",
            "/.well-known/oauth-authorization-server",
        ] {
            assert!(is_reserved(p), "{p}");
        }
        for p in [
            "/",
            "/apiary",
            "/oauthx",
            "/mcpx",
            "/.well-knownx",
            "/printers/mk4",
            "/consent",
            "/login",
        ] {
            assert!(!is_reserved(p), "{p}");
        }
    }

    #[test]
    fn asset_paths() {
        for p in [
            "/_nuxt/app.js",
            "/_nuxt/x",
            "/favicon.ico",
            "/a/b.css",
            "/robots.txt",
        ] {
            assert!(is_asset_path(p), "{p}");
        }
        for p in [
            "/",
            "/printers/mk4",
            "/consent",
            "/users",
            "/printers/mk4-2",
        ] {
            assert!(!is_asset_path(p), "{p}");
        }
    }
}
