//! Blocking HTTP client for the starstar API.

use std::time::Duration;

use reqwest::blocking::{Client, Response};
use reqwest::redirect::Policy;
use serde::de::DeserializeOwned;
use starstar_shared::ErrorBody;
use url::Url;

use crate::endpoints::Endpoint;
use crate::error::{CliError, CliResult};

/// Parses `--api-url`: http or https, no credentials, query or fragment; no trailing slash.
pub fn parse_api_url(s: &str) -> CliResult<Url> {
    let bad = |why: &str| CliError::Usage(format!("invalid API URL `{s}`: {why}"));
    let mut url = Url::parse(s).map_err(|e| bad(&e.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(bad("the scheme must be http or https"));
    }
    if url.host_str().is_none() {
        return Err(bad("it has no host"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(bad("it must not contain a username or password"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(bad("it must not contain a query or fragment"));
    }
    let trimmed = url.path().trim_end_matches('/').to_owned();
    url.set_path(&trimmed);
    Ok(url)
}

/// A reqwest client that never follows redirects and ignores proxy settings.
pub fn build_http() -> CliResult<Client> {
    Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| CliError::Other(format!("could not set up the HTTP client: {e}")))
}

pub struct ApiClient {
    base: Url,
    http: Client,
}

impl ApiClient {
    pub fn unauthenticated(base: Url) -> CliResult<Self> {
        Ok(Self {
            base,
            http: build_http()?,
        })
    }

    pub fn get_json<T: DeserializeOwned>(&self, ep: &Endpoint) -> CliResult<T> {
        let resp = self.send(ep)?;
        resp.json()
            .map_err(|e| CliError::Other(format!("unexpected response from the API: {e}")))
    }

    fn send(&self, ep: &Endpoint) -> CliResult<Response> {
        let resp = self
            .http
            .request(ep.method.clone(), ep.url(&self.base))
            .send()
            .map_err(|e| self.unreachable(&e))?;
        check_status(resp)
    }

    fn unreachable(&self, e: &reqwest::Error) -> CliError {
        CliError::Unreachable {
            url: self.base.to_string(),
            detail: e.to_string(),
        }
    }
}

/// Turns a non-2xx response into [`CliError::Api`].
pub fn check_status(resp: Response) -> CliResult<Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let message = resp
        .json::<ErrorBody>()
        .map(|b| b.error)
        .unwrap_or_else(|_| format!("the API returned HTTP {status}"));
    Err(CliError::Api {
        status: status.as_u16(),
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_api_urls() {
        let u = parse_api_url("http://127.0.0.1:8080/").unwrap();
        assert_eq!(u.as_str(), "http://127.0.0.1:8080/");
        assert_eq!(u.path(), "/");
        assert_eq!(parse_api_url("https://x/sub/").unwrap().path(), "/sub");
        for bad in ["ftp://x", "https://u:p@x", "https://x/?a=1", "https://x/#f"] {
            assert!(
                matches!(parse_api_url(bad), Err(CliError::Usage(_))),
                "{bad}"
            );
        }
    }
}
