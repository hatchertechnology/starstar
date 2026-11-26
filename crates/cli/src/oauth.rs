//! The OAuth protocol steps the CLI needs. Nothing here prints a secret.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::blocking::{Client, Response};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use starstar_shared::auth::{CLI_CLIENT_ID, OAuthErrorBody, TokenResponse};
use starstar_shared::{ErrorBody, Role};
use url::Url;

use crate::credentials::StoredToken;
use crate::endpoints::Endpoint;
use crate::error::{CliError, CliResult};
use crate::session::now;

/// Why a token request failed.
#[derive(Debug)]
pub enum OAuthFailure {
    /// The server refused with an OAuth error body (HTTP 400 or 401).
    Rejected {
        error: String,
        description: Option<String>,
    },
    /// Anything else: network trouble, a 5xx, an unreadable body.
    Other(CliError),
}

impl OAuthFailure {
    /// True when the server says the grant or client is gone for good.
    pub fn is_grant_gone(&self) -> bool {
        matches!(self, Self::Rejected { error, .. } if error == "invalid_grant" || error == "invalid_client")
    }

    pub fn into_error(self) -> CliError {
        match self {
            Self::Rejected { error, description } => CliError::Auth(match description {
                Some(d) => format!("sign-in failed: {error}: {d}"),
                None => format!("sign-in failed: {error}"),
            }),
            Self::Other(e) => e,
        }
    }
}

/// POSTs a token request form and decodes the answer.
pub(crate) fn token_request(
    http: &Client,
    endpoint: &str,
    form: &[(&str, &str)],
) -> Result<TokenResponse, OAuthFailure> {
    let resp = http
        .post(endpoint)
        .form(form)
        .send()
        .map_err(|e| unreachable(endpoint, &e))?;
    let status = resp.status();
    if status.is_success() {
        return resp.json().map_err(|e| {
            OAuthFailure::Other(CliError::Other(format!(
                "unexpected response from the token endpoint: {e}"
            )))
        });
    }
    Err(failure(resp))
}

fn unreachable(endpoint: &str, e: &reqwest::Error) -> OAuthFailure {
    OAuthFailure::Other(CliError::Unreachable {
        url: endpoint.to_owned(),
        detail: e.to_string(),
    })
}

fn failure(resp: Response) -> OAuthFailure {
    let status = resp.status();
    let bytes = resp.bytes().unwrap_or_default();
    if matches!(status.as_u16(), 400 | 401)
        && let Ok(body) = serde_json::from_slice::<OAuthErrorBody>(&bytes)
    {
        return OAuthFailure::Rejected {
            error: body.error,
            description: body.error_description,
        };
    }
    let message = serde_json::from_slice::<ErrorBody>(&bytes)
        .map(|b| b.error)
        .or_else(|_| serde_json::from_slice::<OAuthErrorBody>(&bytes).map(|b| b.error))
        .unwrap_or_else(|_| format!("the server returned HTTP {status}"));
    OAuthFailure::Other(CliError::Api {
        status: status.as_u16(),
        message,
    })
}

/// Exchanges a refresh token for a new pair.
pub fn refresh(
    http: &Client,
    token_endpoint: &str,
    refresh_token: &str,
) -> Result<TokenResponse, OAuthFailure> {
    token_request(
        http,
        token_endpoint,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", CLI_CLIENT_ID),
        ],
    )
}

/// Revokes a refresh token (and with it the whole grant).
pub fn revoke(http: &Client, revocation_endpoint: &str, refresh_token: &str) -> CliResult<()> {
    if let Ok(endpoint) = Url::parse(revocation_endpoint) {
        crate::session::PlainHttpWarner::stderr().check(&endpoint);
    }
    let resp = http
        .post(revocation_endpoint)
        .form(&[
            ("token", refresh_token),
            ("token_type_hint", "refresh_token"),
            ("client_id", CLI_CLIENT_ID),
        ])
        .send()
        .map_err(|e| CliError::Unreachable {
            url: revocation_endpoint.to_owned(),
            detail: e.to_string(),
        })?;
    crate::client::check_status(resp).map(|_| ())
}

/// What `/.well-known/oauth-authorization-server` tells us.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerMetadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub revocation_endpoint: Option<String>,
}

/// Fetches the server metadata. The issuer must be on the same origin as the
/// API we asked, and every endpoint on the issuer's origin, so a hostile or
/// misconfigured document cannot send the sign-in elsewhere.
pub fn discover(http: &Client, api: &Url) -> CliResult<ServerMetadata> {
    let ep = Endpoint {
        method: reqwest::Method::GET,
        segments: vec![".well-known".into(), "oauth-authorization-server".into()],
        query: vec![],
    };
    let resp = http
        .get(ep.url(api))
        .send()
        .map_err(|e| CliError::Unreachable {
            url: api.to_string(),
            detail: e.to_string(),
        })?;
    let resp = crate::client::check_status(resp)?;
    let meta: ServerMetadata = resp.json().map_err(|e| {
        CliError::Other(format!("authorization server metadata is unreadable: {e}"))
    })?;
    let bad = |why: &str| CliError::Other(format!("authorization server metadata {why}"));
    let issuer = Url::parse(&meta.issuer).map_err(|_| bad("has an invalid issuer"))?;
    if !matches!(issuer.scheme(), "http" | "https") {
        return Err(bad("has an invalid issuer"));
    }
    if issuer.origin() != api.origin() {
        return Err(bad("names an issuer on a different origin than the API"));
    }
    let endpoints = [
        Some(&meta.authorization_endpoint),
        Some(&meta.token_endpoint),
        meta.revocation_endpoint.as_ref(),
    ];
    for e in endpoints.into_iter().flatten() {
        let u = Url::parse(e).map_err(|_| bad("has an invalid endpoint"))?;
        if u.origin() != issuer.origin() {
            return Err(bad("has an endpoint on a different origin than the issuer"));
        }
    }
    Ok(meta)
}

/// A PKCE verifier and its S256 challenge. Not `Debug`.
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn new() -> Self {
        Self::from_verifier(&random_b64(32))
    }

    pub fn from_verifier(v: &str) -> Self {
        Self {
            verifier: v.to_owned(),
            challenge: URL_SAFE_NO_PAD.encode(Sha256::digest(v.as_bytes())),
        }
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Self::new()
    }
}

fn random_b64(n: usize) -> String {
    let mut buf = vec![0u8; n];
    getrandom::fill(&mut buf).expect("the operating system has no random numbers");
    URL_SAFE_NO_PAD.encode(buf)
}

pub fn random_state() -> String {
    random_b64(16)
}

/// The URL the user opens to approve the sign-in. There is deliberately no
/// `resource` parameter: it is optional, and the API URL may differ from the
/// public URL.
pub fn authorize_url(
    meta: &ServerMetadata,
    redirect_uri: &str,
    pkce: &Pkce,
    state: &str,
    role: Role,
) -> CliResult<Url> {
    let mut url = Url::parse(&meta.authorization_endpoint).map_err(|_| {
        CliError::Other("authorization server metadata has an invalid endpoint".into())
    })?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CLI_CLIENT_ID)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .append_pair("scope", role.scope());
    Ok(url)
}

/// Trades the authorization code for tokens.
pub fn exchange_code(
    http: &Client,
    meta: &ServerMetadata,
    code: &str,
    redirect_uri: &str,
    pkce: &Pkce,
) -> CliResult<TokenResponse> {
    token_request(
        http,
        &meta.token_endpoint,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", CLI_CLIENT_ID),
            ("code_verifier", &pkce.verifier),
        ],
    )
    .map_err(OAuthFailure::into_error)
}

impl StoredToken {
    pub fn from_response(resp: &TokenResponse, meta: &ServerMetadata) -> Self {
        Self {
            access_token: resp.access_token.clone(),
            refresh_token: resp.refresh_token.clone(),
            expires_at: now() + resp.expires_in as i64,
            scope: resp.scope.clone(),
            issuer: meta.issuer.clone(),
            token_endpoint: meta.token_endpoint.clone(),
            revocation_endpoint: meta.revocation_endpoint.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636() {
        let p = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(p.challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let (a, b) = (Pkce::new(), Pkce::new());
        assert_eq!(a.verifier.len(), 43);
        assert_ne!(a.verifier, b.verifier);
    }

    #[test]
    fn authorize_url_has_required_params() {
        let meta = ServerMetadata {
            issuer: "http://h".into(),
            authorization_endpoint: "http://h/oauth/authorize".into(),
            token_endpoint: "http://h/oauth/token".into(),
            revocation_endpoint: None,
        };
        let p = Pkce::new();
        let url =
            authorize_url(&meta, "http://127.0.0.1:9/callback", &p, "st", Role::Viewer).unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q["response_type"], "code");
        assert_eq!(q["client_id"], "starstar-cli");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["code_challenge"], p.challenge);
        assert_eq!(q["scope"], "starstar:viewer");
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:9/callback");
        assert_eq!(q["state"], "st");
        assert!(!q.contains_key("resource"));
    }
}
