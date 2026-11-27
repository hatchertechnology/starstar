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

pub const TEST_START: i64 = 1_800_000_000;

/// A fresh isolated PostgreSQL state with default config.
pub async fn test_state() -> AppState {
    let db = Db::connect_test().await;
    AppState {
        store: Arc::new(PrinterStore::from_db(db.clone())),
        auth: Arc::new(
            AuthStore::new(db, Clock::manual(TEST_START), Hasher::fast_for_tests())
                .await
                .unwrap(),
        ),
        http: http_client().unwrap(),
        config: Arc::new(Config::default()),
    }
}

/// A router over a fresh isolated PostgreSQL state, plus that state for [`authorize`].
pub async fn test_app() -> (Router, AppState) {
    let state = test_state().await;
    (router(state.clone()), state)
}

/// Creates a user with a real (cheap) password hash.
pub async fn create_user(state: &AppState, username: &str, password: &str, role: Role) {
    let hash = state.auth.hasher().hash(password);
    state.auth.create_user(username, &hash, role).await.unwrap();
}

/// A bearer access token for `username` (created with password
/// `test-password` if missing) through a fresh grant on `starstar-cli`.
pub async fn token_for(state: &AppState, username: &str, role: Role) -> String {
    if state.auth.user_by_name(username).await.unwrap().is_none() {
        create_user(state, username, "test-password", role).await;
    }
    let user = state.auth.user_by_name(username).await.unwrap().unwrap();
    let grant = state
        .auth
        .create_grant(user.id, "starstar-cli", role, None)
        .await
        .unwrap();
    state.auth.issue_tokens(&grant).await.unwrap().access_token
}

static NEXT_USER: AtomicUsize = AtomicUsize::new(0);

/// Adds a real bearer token for a fresh user with `role`.
pub async fn authorize(builder: Builder, state: &AppState, role: Role) -> Builder {
    let n = NEXT_USER.fetch_add(1, Ordering::SeqCst);
    let token = token_for(state, &format!("test-{role}-{n}"), role).await;
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
    pub fn copy(&self) -> Browser {
        Browser {
            cookie: self.cookie.clone(),
            csrf: self.csrf.clone(),
        }
    }

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

/// `(verifier, challenge)` for PKCE S256.
pub fn pkce() -> (String, String) {
    let verifier = starstar_api::auth::crypto::random_token("v");
    let challenge = starstar_api::auth::crypto::pkce_s256(&verifier);
    (verifier, challenge)
}

/// Registers a dynamic client with one redirect URI.
pub async fn register_client(state: &AppState, uri: &str) -> String {
    state
        .auth
        .register_client("Test app", &[uri.to_owned()])
        .await
        .unwrap()
        .id
}

/// A form POST carrying the browser's cookie and CSRF header.
pub async fn browser_form(
    app: &Router,
    browser: &Browser,
    uri: &str,
    form: &[(&str, &str)],
) -> TestResponse {
    let body = form
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let req = browser
        .apply(Request::post(uri))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    send(app, req).await
}

/// Runs authorize + consent as a signed-in browser and returns the code.
pub async fn authorize_code(
    app: &Router,
    browser: &Browser,
    client_id: &str,
    redirect_uri: &str,
    challenge: &str,
    scope: &str,
    role: Role,
) -> String {
    let uri = format!(
        "/oauth/authorize?response_type=code&client_id={client_id}&redirect_uri={}&code_challenge={challenge}&code_challenge_method=S256&state=xyz&scope={}",
        urlencode(redirect_uri),
        urlencode(scope)
    );
    let r = send(
        app,
        browser
            .cookie_only(Request::get(uri))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 303, "{}", r.body);
    let loc = r.headers["location"].to_str().unwrap().to_owned();
    let id = loc
        .strip_prefix("/consent?request=")
        .unwrap_or_else(|| panic!("{loc}"));
    let admin = if role == Role::Admin { "true" } else { "false" };
    let r = browser_form(
        app,
        browser,
        "/oauth/authorize/decision",
        &[
            ("request", id),
            ("decision", "allow"),
            ("role", role.as_str()),
            ("allow_admin", admin),
        ],
    )
    .await;
    assert_eq!(r.status, 303, "{}", r.body);
    let loc = r.headers["location"].to_str().unwrap();
    url::Url::parse(loc)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| panic!("no code in {loc}"))
}
