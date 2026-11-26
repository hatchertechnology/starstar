//! Authentication and authorization.
//!
//! **Interface contract (stable across parts):**
//!
//! - Handlers declare the minimum role they need by taking one of the
//!   extractors [`Viewer`], [`Operator`] or [`Admin`] as a parameter. Each one
//!   rejects the request with 401 (no or bad credentials) or 403 (role too low)
//!   before the handler runs, and gives access to the authenticated [`Caller`].
//! - Non-handler code (MCP tools) calls [`Caller::require`] with the role it needs.
//! - [`router`] holds the auth-owned routes (setup, session, OAuth, users, apps)
//!   and is merged into the main router by [`crate::router`].
//! - [`require_auth`] denies every `/api/*` and `/mcp` request that is not
//!   authenticated (except a small allowlist) and puts the [`Caller`] into the
//!   request extensions, where the extractors find it.

pub mod apps;
pub mod client_ip;
pub mod clock;
pub mod cookies;
pub mod crypto;
pub mod oauth;
pub mod password;
pub mod ratelimit;
pub mod session;
pub mod setup;
pub mod startup;
pub mod store;
pub mod users;

pub use store::AuthStore;
use store::AuthStoreError;

use axum::Router;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use starstar_shared::Role;
use starstar_shared::auth::{CSRF_HEADER, SESSION_COOKIE};
use store::sessions::SessionCaller;

use crate::AppState;
use crate::error::ApiError;

/// The authenticated identity behind a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    /// Username of the signed-in user.
    pub username: String,
    /// Effective role: the lower of the token's grant and the user's current role.
    pub role: Role,
}

impl Caller {
    /// Returns 403 unless this caller has at least `min`.
    pub fn require(&self, min: Role) -> Result<(), ApiError> {
        if self.role >= min {
            Ok(())
        } else {
            Err(ApiError::InsufficientRole(format!(
                "this action needs the {min} role, you have {}",
                self.role
            )))
        }
    }
}

/// Resolves the caller from the request: a bearer token if an
/// `Authorization` header is present (no fallback to cookies), otherwise a
/// browser session.
pub async fn authenticate(parts: &mut Parts, state: &AppState) -> Result<Caller, ApiError> {
    if let Some(value) = parts.headers.get(header::AUTHORIZATION) {
        let token = value
            .to_str()
            .ok()
            .and_then(|v| v.split_once(' '))
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
            .map(|(_, token)| token.trim());
        let invalid = || ApiError::Unauthorized("invalid or expired access token".to_owned());
        let token = token.filter(|t| !t.is_empty()).ok_or_else(invalid)?;
        return state
            .auth
            .access_token_caller(token)
            .await?
            .ok_or_else(invalid);
    }
    let session = session_from_parts(parts, state).await?;
    if !matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS) {
        let supplied = parts
            .headers
            .get(CSRF_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !crypto::ct_eq(supplied.as_bytes(), session.csrf_token.as_bytes()) {
            return Err(ApiError::Forbidden(
                "missing or invalid CSRF token".to_owned(),
            ));
        }
    }
    let caller = session.caller.clone();
    parts.extensions.insert(session);
    Ok(caller)
}

/// The browser session from the cookie alone, with no CSRF check. For
/// handlers that do their own CSRF check or only read.
pub(crate) async fn session_from_parts(
    parts: &Parts,
    state: &AppState,
) -> Result<SessionCaller, ApiError> {
    match cookies::get_cookie(&parts.headers, SESSION_COOKIE) {
        Some(token) => state.auth.session_caller(&token).await?,
        None => None,
    }
    .ok_or_else(|| ApiError::Unauthorized("sign in required".to_owned()))
}

macro_rules! role_extractor {
    ($name:ident, $role:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone)]
        pub struct $name(pub Caller);

        impl FromRequestParts<AppState> for $name {
            type Rejection = ApiError;

            async fn from_request_parts(
                parts: &mut Parts,
                state: &AppState,
            ) -> Result<Self, Self::Rejection> {
                let caller = match parts.extensions.get::<Caller>() {
                    Some(caller) => caller.clone(),
                    None => authenticate(parts, state).await?,
                };
                caller.require($role)?;
                Ok(Self(caller))
            }
        }
    };
}

role_extractor!(Viewer, Role::Viewer, "Requires at least the viewer role.");
role_extractor!(
    Operator,
    Role::Operator,
    "Requires at least the operator role."
);
role_extractor!(Admin, Role::Admin, "Requires the admin role.");

/// URL of the protected-resource metadata document that applies to `path`.
/// `/mcp` has its own document, whose `resource` is `{public_url}/mcp`.
pub fn protected_resource_metadata_url(state: &AppState, path: &str) -> String {
    let base = &state.config.public_url;
    if path == "/mcp" || path.starts_with("/mcp/") {
        format!("{base}/.well-known/oauth-protected-resource/mcp")
    } else {
        format!("{base}/.well-known/oauth-protected-resource")
    }
}

/// `/api/*` routes that need no credentials.
pub fn is_allowlisted(method: &Method, path: &str) -> bool {
    match path {
        "/api/health" => method == Method::GET || method == Method::HEAD,
        "/api/setup" | "/api/session" => method == Method::POST,
        _ => false,
    }
}

/// The path as a router or proxy might see it after normalization: repeated
/// `/` collapsed, percent-escapes of unreserved characters (letters, digits,
/// `-`, `.`, `_`, `~`) decoded, and `.` / `..` segments resolved. Other
/// escapes (such as `%2F`) are left as they are.
pub fn normalize_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(byte) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            && (byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~'))
        {
            decoded.push(byte);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    let decoded = String::from_utf8_lossy(&decoded);
    let mut segments: Vec<&str> = Vec::new();
    for segment in decoded.split('/').filter(|s| !s.is_empty()) {
        match segment {
            "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    format!("/{}", segments.join("/"))
}

fn is_guarded(path: &str) -> bool {
    let guarded =
        |p: &str| p == "/api" || p.starts_with("/api/") || p == "/mcp" || p.starts_with("/mcp/");
    guarded(path) || guarded(&normalize_path(path))
}

/// Adds the `WWW-Authenticate` challenge to 401 responses (with
/// `error="invalid_token"` when the request carried a bearer token that was
/// rejected) and completes the `insufficient_scope` challenge on role 403s.
fn with_challenge(
    mut response: Response,
    state: &AppState,
    path: &str,
    invalid_token: bool,
) -> Response {
    let metadata = protected_resource_metadata_url(state, path);
    let challenge = match response.status() {
        StatusCode::UNAUTHORIZED if !response.headers().contains_key(header::WWW_AUTHENTICATE) => {
            if invalid_token {
                format!("Bearer error=\"invalid_token\", resource_metadata=\"{metadata}\"")
            } else {
                format!("Bearer resource_metadata=\"{metadata}\"")
            }
        }
        StatusCode::FORBIDDEN
            if response
                .headers()
                .get(header::WWW_AUTHENTICATE)
                .is_some_and(|v| v == crate::error::INSUFFICIENT_SCOPE_CHALLENGE) =>
        {
            format!(
                "{}, resource_metadata=\"{metadata}\"",
                crate::error::INSUFFICIENT_SCOPE_CHALLENGE
            )
        }
        _ => return response,
    };
    if let Ok(value) = HeaderValue::from_str(&challenge) {
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, value);
    }
    response
}

/// Whether the request presented a bearer token (valid or not).
fn presented_bearer(headers: &axum::http::HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
}

/// Default deny for `/api/*` and `/mcp`: every request must authenticate,
/// except the allowlist. The caller is put into the request extensions.
/// Anything else (`/oauth/*`, `/.well-known/*`, the web UI) passes through.
pub async fn require_auth(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let raw_path = req.uri().path().to_owned();
    if !is_guarded(&raw_path) {
        return next.run(req).await;
    }
    // The allowlist matches the raw path exactly; every other decision (guard,
    // challenge) looks at the normalized one.
    let path = normalize_path(&raw_path);
    if is_allowlisted(req.method(), &raw_path) {
        return with_challenge(next.run(req).await, &state, &path, false);
    }
    let bearer = presented_bearer(req.headers());
    let (mut parts, body) = req.into_parts();
    match authenticate(&mut parts, &state).await {
        Ok(caller) => {
            parts.extensions.insert(caller);
            let req = Request::from_parts(parts, body);
            with_challenge(next.run(req).await, &state, &path, false)
        }
        Err(err) => with_challenge(err.into_response(), &state, &path, bearer),
    }
}

/// Forbids framing on every response. Handlers that set their own
/// `Content-Security-Policy` must include `frame-ancestors 'none'`.
pub async fn security_headers(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    if !headers.contains_key(header::CONTENT_SECURITY_POLICY) {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("frame-ancestors 'none'"),
        );
    }
    response
}

/// Auth-owned routes (setup, session, users, apps, OAuth).
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(session::routes())
        .merge(setup::routes())
        .merge(users::routes())
        .merge(apps::routes())
        .merge(oauth::routes())
        .with_state(state)
}

impl From<AuthStoreError> for ApiError {
    fn from(err: AuthStoreError) -> Self {
        match err {
            AuthStoreError::UsernameTaken(_)
            | AuthStoreError::LastAdmin
            | AuthStoreError::SetupClosed => Self::Conflict(err.to_string()),
            AuthStoreError::UserNotFound(_) => Self::NotFound(err.to_string()),
            AuthStoreError::Busy => Self::ServiceUnavailable(err.to_string()),
            _ => Self::Internal("internal error".to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_normalized() {
        for (raw, want) in [
            ("/api/printers", "/api/printers"),
            ("//api//printers/", "/api/printers"),
            ("/%61pi/%50rinters", "/api/Printers"),
            ("/api/%2e%2E/mcp", "/mcp"),
            ("/a/./b/../c", "/a/c"),
            ("/api%2Fprinters", "/api%2Fprinters"),
            ("/%zz/%4", "/%zz/%4"),
            ("/", "/"),
            ("", "/"),
            ("/../..", "/"),
        ] {
            assert_eq!(normalize_path(raw), want, "{raw}");
        }
    }
}
