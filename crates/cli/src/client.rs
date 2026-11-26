//! Blocking HTTP client for the starstar API.

use std::fs::File;
use std::path::Path;
use std::time::Duration;

use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::HeaderMap;
use reqwest::redirect::Policy;
use serde::Serialize;
use serde::de::DeserializeOwned;
use starstar_shared::ErrorBody;
use url::Url;

use crate::credentials::{CredentialStore, default_path, server_key};
use crate::endpoints::Endpoint;
use crate::error::{CliError, CliResult};
use crate::session::{PlainHttpWarner, TokenOrigin, TokenSource};

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
    tokens: Option<TokenSource>,
    warner: &'static PlainHttpWarner,
}

/// How a request builder is filled in for each attempt (the 401 retry included).
type Build<'a> = &'a dyn Fn(RequestBuilder) -> CliResult<RequestBuilder>;

impl ApiClient {
    pub fn new(
        base: Url,
        tokens: Option<TokenSource>,
        http: Client,
        warner: &'static PlainHttpWarner,
    ) -> Self {
        Self {
            base,
            http,
            tokens,
            warner,
        }
    }

    /// A client that sends no credential (health check, discovery).
    pub fn unauthenticated(base: Url) -> CliResult<Self> {
        Ok(Self::new(
            base,
            None,
            build_http()?,
            PlainHttpWarner::stderr(),
        ))
    }

    /// A client signed in from `STARSTAR_TOKEN`, or else from `credentials.json`.
    pub fn from_env(base: Url) -> CliResult<Self> {
        let http = build_http()?;
        let warner = PlainHttpWarner::stderr();
        let tokens = match std::env::var("STARSTAR_TOKEN")
            .ok()
            .filter(|t| !t.is_empty())
        {
            Some(t) => TokenSource::env(base.clone(), t),
            None => TokenSource::stored(
                base.clone(),
                CredentialStore::at(default_path()?),
                http.clone(),
                warner,
            )?,
        };
        Ok(Self::new(base, Some(tokens), http, warner))
    }

    pub fn http(&self) -> &Client {
        &self.http
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    pub fn tokens(&self) -> Option<&TokenSource> {
        self.tokens.as_ref()
    }

    /// Sends the request with a bearer token. A 401 triggers one refresh and
    /// one resend. With `retry_transport`, a transport error after the
    /// credential was sent does the same.
    fn execute(
        &self,
        ep: &Endpoint,
        build: Build<'_>,
        margin: i64,
        retry_transport: bool,
    ) -> CliResult<Response> {
        let url = ep.url(&self.base);
        let mut token = match &self.tokens {
            Some(t) => Some(t.bearer_with_margin(margin)?),
            None => None,
        };
        let mut retried = false;
        loop {
            let mut rb = self.http.request(ep.method.clone(), url.clone());
            if let Some(t) = &token {
                self.warner.check(&self.base);
                rb = rb.bearer_auth(t);
            }
            match build(rb)?.send() {
                Ok(resp) => {
                    if resp.status().as_u16() == 401
                        && !retried
                        && let (Some(src), Some(t)) = (&self.tokens, &token)
                        && let Some(new) = src.after_rejection(t)?
                    {
                        token = Some(new);
                        retried = true;
                        continue;
                    }
                    return self.finish(resp);
                }
                Err(e) => {
                    if retry_transport
                        && !retried
                        && let (Some(src), Some(t)) = (&self.tokens, &token)
                        && let Ok(Some(new)) = src.after_rejection(t)
                    {
                        token = Some(new);
                        retried = true;
                        continue;
                    }
                    return Err(CliError::Unreachable {
                        url: self.base.to_string(),
                        detail: e.to_string(),
                    });
                }
            }
        }
    }

    fn finish(&self, resp: Response) -> CliResult<Response> {
        match check_status(resp) {
            Err(CliError::Api {
                status: 401,
                message,
            }) => Err(match self.tokens.as_ref().map(TokenSource::origin) {
                Some(TokenOrigin::Env) => CliError::Auth(format!(
                    "STARSTAR_TOKEN was rejected by {} (expired or revoked)",
                    server_key(&self.base)
                )),
                Some(TokenOrigin::Stored) => {
                    CliError::Auth(format!("{message}; run `starstar login`"))
                }
                None => CliError::Api {
                    status: 401,
                    message,
                },
            }),
            other => other,
        }
    }

    fn plain(&self, ep: &Endpoint) -> CliResult<Response> {
        self.execute(ep, &|rb| Ok(rb), 60, false)
    }

    pub fn get_json<T: DeserializeOwned>(&self, ep: &Endpoint) -> CliResult<T> {
        decode(self.plain(ep)?)
    }

    /// Like [`ApiClient::get_json`], but 204 means `None`.
    pub fn get_optional<T: DeserializeOwned>(&self, ep: &Endpoint) -> CliResult<Option<T>> {
        let resp = self.plain(ep)?;
        if resp.status().as_u16() == 204 {
            return Ok(None);
        }
        decode(resp).map(Some)
    }

    pub fn send_json<B: Serialize, T: DeserializeOwned>(
        &self,
        ep: &Endpoint,
        body: &B,
    ) -> CliResult<T> {
        decode(self.execute(ep, &|rb| Ok(rb.json(body)), 60, false)?)
    }

    /// Sends an optional JSON body and ignores the response body.
    pub fn send_empty(&self, ep: &Endpoint, body: Option<&serde_json::Value>) -> CliResult<()> {
        self.execute(
            ep,
            &|rb| {
                Ok(match body {
                    Some(b) => rb.json(b),
                    None => rb,
                })
            },
            60,
            false,
        )?;
        Ok(())
    }

    /// Streams `file` as the request body (`Content-Length` set, nothing
    /// buffered). Each attempt reopens the file. A token that expires within
    /// 120 s is refreshed first, and a rejected or interrupted send is retried
    /// once with a fresh token. Returns the size of the file in bytes.
    pub fn upload(&self, ep: &Endpoint, file: &Path) -> CliResult<u64> {
        let len = std::fs::metadata(file)?.len();
        self.execute(
            ep,
            &|rb| {
                let f = File::open(file)?;
                let len = f.metadata()?.len();
                Ok(rb
                    .timeout(Duration::from_secs(3600))
                    .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
                    .body(reqwest::blocking::Body::sized(f, len)))
            },
            120,
            true,
        )?;
        Ok(len)
    }

    pub fn head(&self, ep: &Endpoint) -> CliResult<HeaderMap> {
        Ok(self.plain(ep)?.headers().clone())
    }
}

fn decode<T: DeserializeOwned>(resp: Response) -> CliResult<T> {
    resp.json()
        .map_err(|e| CliError::Other(format!("unexpected response from the API: {e}")))
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
