//! `GET /oauth/authorize`, the consent hand-off, the decision endpoint and the
//! copy-code page.

use axum::extract::{Path, Query, State};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, LOCATION, REFERRER_POLICY,
    X_FRAME_OPTIONS,
};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use serde::Deserialize;
use starstar_shared::Role;
use starstar_shared::auth::{CSRF_HEADER, ConsentRequest};
use url::Url;

use super::OAuthError;
use super::redirect::redirect_allowed;
use crate::AppState;
use crate::auth::crypto::{ct_eq, is_valid_pkce_challenge};
use crate::auth::session_from_parts;
use crate::auth::store::codes::{CodeParams, PendingAuthorization, requested_role};
use crate::error::ApiError;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/authorize/requests/{id}", get(consent_request))
        .route("/oauth/authorize/decision", post(decision))
        .route("/oauth/code", get(copy_code))
}

#[derive(Deserialize)]
struct AuthorizeQuery {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    state: Option<String>,
    scope: Option<String>,
    resource: Option<String>,
}

fn see_other(location: &str) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    if let Ok(value) = HeaderValue::from_str(location) {
        response.headers_mut().insert(LOCATION, value);
    }
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// A 303 to `redirect_uri` with the given query parameters (and `state`).
fn redirect_with(
    redirect_uri: &str,
    state: Option<&str>,
    params: &[(&str, &str)],
) -> Result<Response, OAuthError> {
    let mut url = Url::parse(redirect_uri).map_err(|_| OAuthError::server_error())?;
    {
        let mut q = url.query_pairs_mut();
        for (k, v) in params {
            q.append_pair(k, v);
        }
        if let Some(state) = state {
            q.append_pair("state", state);
        }
    }
    Ok(see_other(url.as_str()))
}

async fn authorize(
    State(state): State<AppState>,
    parts: Parts,
    Query(q): Query<AuthorizeQuery>,
) -> Result<Response, OAuthError> {
    // Errors before the redirect URI is trusted are never redirected.
    let client_id = q
        .client_id
        .as_deref()
        .ok_or_else(|| OAuthError::invalid_request("client_id is required"))?;
    let client = state
        .auth
        .client(client_id)?
        .ok_or_else(|| OAuthError::invalid_request("unknown client_id"))?;
    let redirect_uri = q
        .redirect_uri
        .as_deref()
        .ok_or_else(|| OAuthError::invalid_request("redirect_uri is required"))?;
    if !redirect_allowed(
        &client,
        redirect_uri,
        &state.config.public_url,
        &state.config.bind,
    ) {
        return Err(OAuthError::invalid_request(
            "redirect_uri is not registered for this client",
        ));
    }

    let st = q.state.as_deref();
    let fail = |error: &str, description: &str| {
        redirect_with(
            redirect_uri,
            st,
            &[("error", error), ("error_description", description)],
        )
    };
    match q.response_type.as_deref() {
        None => return fail("invalid_request", "response_type is required"),
        Some("code") => {}
        Some(_) => {
            return fail(
                "unsupported_response_type",
                "only response_type=code is supported",
            );
        }
    }
    let challenge = match q.code_challenge.as_deref() {
        Some(c) if is_valid_pkce_challenge(c) => c,
        _ => return fail("invalid_request", "a valid S256 code_challenge is required"),
    };
    if q.code_challenge_method.as_deref() != Some("S256") {
        return fail("invalid_request", "code_challenge_method must be S256");
    }
    let public = &state.config.public_url;
    if let Some(resource) = q.resource.as_deref()
        && resource != public
        && !resource.starts_with(&format!("{public}/"))
    {
        return fail("invalid_target", "resource is not served by this server");
    }

    let id = state
        .auth
        .put_pending(PendingAuthorization {
            client_id: client.id.clone(),
            redirect_uri: redirect_uri.to_owned(),
            code_challenge: challenge.to_owned(),
            state: q.state.clone(),
            requested_role: requested_role(q.scope.as_deref()),
            resource: q.resource.clone(),
            expires_at: 0,
        })
        .ok_or_else(OAuthError::temporarily_unavailable)?;

    let consent = format!("/consent?request={id}");
    if session_from_parts(&parts, &state).is_ok() {
        Ok(see_other(&consent))
    } else {
        let next: String = url::form_urlencoded::byte_serialize(consent.as_bytes()).collect();
        Ok(see_other(&format!("/login?next={next}")))
    }
}

async fn consent_request(
    State(state): State<AppState>,
    parts: Parts,
    Path(id): Path<String>,
) -> Result<Json<ConsentRequest>, ApiError> {
    let session = session_from_parts(&parts, &state)?;
    let not_found = || ApiError::NotFound("no such authorization request".to_owned());
    let pending = state.auth.pending(&id).ok_or_else(not_found)?;
    let client = state
        .auth
        .client(&pending.client_id)?
        .ok_or_else(not_found)?;
    let user_role = session.caller.role;
    let redirect_host = Url::parse(&pending.redirect_uri)
        .ok()
        .and_then(|u| {
            u.host_str().map(|h| {
                u.port()
                    .map_or_else(|| h.to_owned(), |p| format!("{h}:{p}"))
            })
        })
        .unwrap_or_default();
    Ok(Json(ConsentRequest {
        id,
        client_id: client.id,
        client_name: client.name,
        client_name_self_declared: !client.seeded,
        redirect_host,
        client_registered_at: client.created_at,
        requested_role: pending.requested_role,
        user_role,
        default_role: pending.requested_role.min(Role::Operator).min(user_role),
        max_role: pending.requested_role.min(user_role),
    }))
}

#[derive(Deserialize)]
struct Decision {
    request: String,
    decision: String,
    role: Option<String>,
    csrf_token: Option<String>,
    /// Admin is granted only with an explicit `allow_admin=true`.
    allow_admin: Option<String>,
}

async fn decision(
    State(state): State<AppState>,
    parts: Parts,
    Form(form): Form<Decision>,
) -> Result<Response, ApiError> {
    let session = session_from_parts(&parts, &state)?;
    let supplied = parts
        .headers
        .get(CSRF_HEADER)
        .and_then(|v| v.to_str().ok())
        .or(form.csrf_token.as_deref())
        .unwrap_or("");
    if !ct_eq(supplied.as_bytes(), session.csrf_token.as_bytes()) {
        return Err(ApiError::Forbidden(
            "missing or invalid CSRF token".to_owned(),
        ));
    }
    let pending = state
        .auth
        .take_pending(&form.request)
        .ok_or_else(|| ApiError::NotFound("no such authorization request".to_owned()))?;
    let oauth = |e: OAuthError| ApiError::Internal(e.description);

    match form.decision.as_str() {
        "deny" => redirect_with(
            &pending.redirect_uri,
            pending.state.as_deref(),
            &[("error", "access_denied")],
        )
        .map_err(oauth),
        "allow" => {
            let role: Role = form
                .role
                .as_deref()
                .ok_or_else(|| ApiError::BadRequest("role is required".to_owned()))?
                .parse()
                .map_err(|e: starstar_shared::UnknownRole| ApiError::BadRequest(e.to_string()))?;
            let ceiling = pending.requested_role.min(session.caller.role);
            if role > ceiling {
                return Err(ApiError::BadRequest(format!(
                    "the {role} role is above what this request and your account allow ({ceiling})"
                )));
            }
            let allow_admin = matches!(form.allow_admin.as_deref(), Some("true" | "on" | "1"));
            if role == Role::Admin && !allow_admin {
                return Err(ApiError::BadRequest(
                    "granting admin needs allow_admin=true".to_owned(),
                ));
            }
            let code = state.auth.create_code(&CodeParams {
                client_id: &pending.client_id,
                user_id: session.user_id,
                role,
                redirect_uri: &pending.redirect_uri,
                code_challenge: &pending.code_challenge,
                resource: pending.resource.as_deref(),
            })?;
            redirect_with(
                &pending.redirect_uri,
                pending.state.as_deref(),
                &[("code", &code)],
            )
            .map_err(oauth)
        }
        _ => Err(ApiError::BadRequest(
            "decision must be allow or deny".to_owned(),
        )),
    }
}

#[derive(Deserialize)]
struct CodeQuery {
    code: Option<String>,
}

fn escape_html(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&#39;".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

fn page(status: StatusCode, body: String) -> Response {
    let mut response = (status, Html(body)).into_response();
    let h = response.headers_mut();
    h.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    h.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'none'",
        ),
    );
    response
}

async fn copy_code(
    State(state): State<AppState>,
    parts: Parts,
    Query(q): Query<CodeQuery>,
) -> Response {
    let Ok(session) = session_from_parts(&parts, &state) else {
        return page(
            StatusCode::UNAUTHORIZED,
            "<!doctype html><meta charset=utf-8><title>starstar</title><p>Sign in first, then try again.</p>"
                .to_owned(),
        );
    };
    let Some(code) = q.code else {
        return page(
            StatusCode::BAD_REQUEST,
            "<!doctype html><meta charset=utf-8><title>starstar</title><p>No code in this link.</p>"
                .to_owned(),
        );
    };
    // Only the signed-in user's own live CLI code is ever shown; anything else
    // gets the same generic page, so the page cannot be used to display text
    // chosen by a link.
    if !state
        .auth
        .code_shown_to(&code, session.user_id)
        .unwrap_or(false)
    {
        return page(
            StatusCode::NOT_FOUND,
            "<!doctype html><meta charset=utf-8><title>starstar</title><p>This code is not available. Run starstar login again.</p>"
                .to_owned(),
        );
    }
    page(
        StatusCode::OK,
        format!(
            "<!doctype html><meta charset=utf-8><title>starstar: your code</title>\
             <style>body{{font-family:system-ui,sans-serif;max-width:40rem;margin:3rem auto;padding:0 1rem}}\
             code{{display:block;padding:1rem;background:#eee;word-break:break-all;font-size:1.1rem}}</style>\
             <h1>Your authorization code</h1><code id=\"code\">{}</code>\
             <p>Paste this code only into your own terminal, where you ran starstar login.</p>",
            escape_html(&code)
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::escape_html;

    #[test]
    fn html_is_escaped() {
        assert_eq!(escape_html("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&#39;");
    }
}
