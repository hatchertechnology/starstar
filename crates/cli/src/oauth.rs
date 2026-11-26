//! The OAuth protocol steps the CLI needs. Nothing here prints a secret.

use reqwest::blocking::{Client, Response};
use starstar_shared::ErrorBody;
use starstar_shared::auth::{CLI_CLIENT_ID, OAuthErrorBody, TokenResponse};

use crate::error::CliError;

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
