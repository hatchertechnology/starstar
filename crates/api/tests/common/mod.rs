//! Helpers shared by the API integration tests.
//!
//! **Interface contract:** integration tests build their app with
//! [`test_app`] and add credentials with [`authorize`], so they keep working
//! with real auth. [`authorize`] creates a user with a unique username and
//! `role` in `state` (cheap argon2 parameters) and attaches a real bearer token.
#![allow(dead_code)]

use std::sync::Arc;

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::body::Body;
use axum::http::request::Builder;
use axum::http::{HeaderMap, Request, StatusCode};
use starstar_api::auth::AuthStore;
use starstar_api::auth::clock::Clock;
use starstar_api::auth::password::Hasher;
use starstar_api::config::Config;
use starstar_api::prusalink::http_client;
use starstar_api::store::{Db, PrinterStore};
use starstar_api::{AppState, router};
use starstar_shared::Role;
use tower::ServiceExt;

/// A fresh in-memory state with default config.
pub const TEST_START: i64 = 1_800_000_000;

pub fn test_state() -> AppState {
    let db = Db::open_in_memory().unwrap();
    AppState {
        store: Arc::new(PrinterStore::from_db(db.clone())),
        auth: Arc::new(
            AuthStore::new(db, Clock::manual(TEST_START), Hasher::fast_for_tests()).unwrap(),
        ),
        http: http_client().unwrap(),
        config: Arc::new(Config::default()),
    }
}

/// A router over a fresh in-memory state, plus that state for [`authorize`].
pub fn test_app() -> (Router, AppState) {
    let state = test_state();
    (router(state.clone()), state)
}

/// Creates a user with a real (cheap) password hash.
pub fn create_user(state: &AppState, username: &str, password: &str, role: Role) {
    let hash = state.auth.hasher().hash(password);
    state.auth.create_user(username, &hash, role).unwrap();
}

/// A bearer access token for `username` (created with password
/// `test-password` if missing) through a fresh grant on `starstar-cli`.
pub fn token_for(state: &AppState, username: &str, role: Role) -> String {
    if state.auth.user_by_name(username).unwrap().is_none() {
        create_user(state, username, "test-password", role);
    }
    let user = state.auth.user_by_name(username).unwrap().unwrap();
    let grant = state
        .auth
        .create_grant(user.id, "starstar-cli", role, None)
        .unwrap();
    state.auth.issue_tokens(&grant).unwrap().access_token
}

static NEXT_USER: AtomicUsize = AtomicUsize::new(0);

/// Adds a real bearer token for a fresh user with `role`.
pub fn authorize(builder: Builder, state: &AppState, role: Role) -> Builder {
    let n = NEXT_USER.fetch_add(1, Ordering::SeqCst);
    let token = token_for(state, &format!("test-{role}-{n}"), role);
    builder.header("authorization", format!("Bearer {token}"))
}

/// A response with its body read into a string.
pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: String,
}

impl TestResponse {
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("not JSON ({e}): {}", self.body))
    }
}

pub async fn send(app: &Router, req: Request<Body>) -> TestResponse {
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    TestResponse {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

pub async fn post_json(app: &Router, uri: &str, body: serde_json::Value) -> TestResponse {
    let req = Request::post(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

pub fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

pub async fn post_form(app: &Router, uri: &str, form: &[(&str, &str)]) -> TestResponse {
    let body = form
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let req = Request::post(uri)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    send(app, req).await
}

/// A signed-in browser: the cookie header value and its CSRF token.
pub struct Browser {
    pub cookie: String,
    pub csrf: String,
}

impl Browser {
    /// Adds the cookie and the CSRF header.
    pub fn apply(&self, b: Builder) -> Builder {
        self.cookie_only(b)
            .header("x-csrf-token", self.csrf.as_str())
    }

    /// Adds only the cookie.
    pub fn cookie_only(&self, b: Builder) -> Builder {
        b.header("cookie", self.cookie.as_str())
    }
}

/// Signs in through `POST /api/session` and returns the browser state.
pub async fn login(app: &Router, username: &str, password: &str) -> Browser {
    let r = post_json(
        app,
        "/api/session",
        serde_json::json!({"username": username, "password": password}),
    )
    .await;
    assert_eq!(r.status, 200, "login failed: {}", r.body);
    browser_from(&r)
}

/// Builds a [`Browser`] from the `Set-Cookie` headers of a response.
pub fn browser_from(r: &TestResponse) -> Browser {
    let pairs: Vec<String> = r
        .headers
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned())
        .collect();
    Browser {
        cookie: pairs.join("; "),
        csrf: r.json()["csrf_token"].as_str().unwrap().to_owned(),
    }
}
