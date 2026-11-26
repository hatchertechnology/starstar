//! `POST /oauth/token` and `POST /oauth/revoke`.

use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, PRAGMA};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Form, Json, Router};
use serde::Deserialize;

use super::OAuthError;
use crate::AppState;
use crate::auth::client_ip::client_ip;
use crate::auth::store::GrantError;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/oauth/token", post(token))
        .route("/oauth/revoke", post(revoke))
}

#[derive(Deserialize)]
struct TokenForm {
    grant_type: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
}

impl From<GrantError> for OAuthError {
    fn from(e: GrantError) -> Self {
        match e {
            GrantError::Invalid(m) => OAuthError::invalid_grant(m),
            GrantError::Store(_) => OAuthError::server_error(),
        }
    }
}

fn required(value: Option<String>, name: &str) -> Result<String, OAuthError> {
    value
        .filter(|v| !v.is_empty())
        .ok_or_else(|| OAuthError::invalid_request(format!("{name} is required")))
}

async fn token(
    State(state): State<AppState>,
    parts: Parts,
    Form(form): Form<TokenForm>,
) -> Result<Response, OAuthError> {
    let auth = state.auth.clone();
    let ip = client_ip(&parts, &state.config.trusted_proxies);
    let now = auth.now();
    if auth.token_limiter().is_blocked(&ip, now) {
        return Err(OAuthError::too_many_requests());
    }
    let result = issue(&state, form).await;
    // Failed client or grant checks count against the caller's address.
    // Success deliberately does not clear it: one valid account must not be
    // able to reset an attacker's budget.
    if matches!(&result, Err(e) if e.error == "invalid_client" || e.error == "invalid_grant") {
        auth.token_limiter().record_failure(&ip, now);
    }
    let tokens = result?;
    let mut response = Json(tokens.into_response_body()).into_response();
    let h = response.headers_mut();
    h.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    Ok(response)
}

async fn issue(
    state: &AppState,
    form: TokenForm,
) -> Result<crate::auth::store::IssuedTokens, OAuthError> {
    let client_id = required(form.client_id, "client_id")?;
    if state.auth.client(&client_id)?.is_none() {
        return Err(OAuthError::invalid_client("unknown client"));
    }
    match form.grant_type.as_deref() {
        Some("authorization_code") => {
            let code = required(form.code, "code")?;
            let redirect_uri = required(form.redirect_uri, "redirect_uri")?;
            let verifier = required(form.code_verifier, "code_verifier")?;
            Ok(state
                .auth
                .exchange_code(&code, &client_id, &redirect_uri, &verifier)?)
        }
        Some("refresh_token") => {
            let refresh_token = required(form.refresh_token, "refresh_token")?;
            Ok(state.auth.refresh(&refresh_token, &client_id)?)
        }
        _ => Err(OAuthError::unsupported_grant_type(
            "grant_type must be authorization_code or refresh_token",
        )),
    }
}

#[derive(Deserialize)]
struct RevokeForm {
    token: Option<String>,
    client_id: Option<String>,
}

async fn revoke(
    State(state): State<AppState>,
    Form(form): Form<RevokeForm>,
) -> Result<Response, OAuthError> {
    let token = required(form.token, "token")?;
    state.auth.revoke_token(&token, form.client_id.as_deref())?;
    Ok(StatusCode::OK.into_response())
}
