//! Client for the PrusaLink REST API that Prusa printers expose on the LAN.
//!
//! The endpoints used here are described in `docs/prusalink-openapi.yaml`.
//! Every endpoint requires HTTP Digest authentication.

use std::time::Duration;

use axum::body::Bytes;
use diqwest::WithDigestAuth;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;

mod digest;
pub mod wire;

pub use wire::*;

#[derive(Debug, thiserror::Error)]
pub enum PrusaError {
    #[error("could not reach printer: {0}")]
    Transport(String),
    #[error("printer did not respond in time")]
    Timeout,
    #[error("printer responded with HTTP {status}")]
    Status {
        status: StatusCode,
        detail: Option<String>,
    },
    #[error("unexpected response from printer: {0}")]
    Decode(String),
}

/// Builds the HTTP client shared by all printer requests.
pub fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
}

/// A client bound to one printer's address and credentials.
///
/// Not `Debug` on purpose: it holds the printer password.
pub struct PrusaLinkClient {
    http: reqwest::Client,
    host: String,
    username: String,
    password: String,
    request_timeout: Duration,
    upload_timeout: Duration,
}

fn map_reqwest(err: reqwest::Error) -> PrusaError {
    if err.is_timeout() {
        PrusaError::Timeout
    } else {
        PrusaError::Transport(err.without_url().to_string())
    }
}

fn map_digest(err: diqwest::error::Error) -> PrusaError {
    match err {
        diqwest::error::Error::Reqwest(e) => map_reqwest(e),
        other => PrusaError::Transport(other.to_string()),
    }
}

/// `text` of a JSON `{title,text}` body, else the trimmed plain text cut to 200 chars.
fn error_detail(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Problem {
        text: Option<String>,
    }
    if let Ok(Problem { text: Some(text) }) = serde_json::from_slice(body) {
        return Some(text).filter(|t| !t.trim().is_empty());
    }
    let text = String::from_utf8_lossy(body);
    let text = text.trim();
    if text.is_empty() || text.starts_with('{') || text.starts_with('<') {
        None
    } else {
        Some(text.chars().take(200).collect())
    }
}

impl PrusaLinkClient {
    pub fn new(http: reqwest::Client, host: &str, username: &str, password: &str) -> Self {
        Self {
            http,
            host: host.to_owned(),
            username: username.to_owned(),
            password: password.to_owned(),
            request_timeout: Duration::from_secs(10),
            upload_timeout: Duration::from_secs(3600),
        }
    }

    pub fn with_timeouts(mut self, request: Duration, upload: Duration) -> Self {
        self.request_timeout = request;
        self.upload_timeout = upload;
        self
    }

    /// `http://{host}/` plus each segment, pushed with `path_segments_mut` so it
    /// is percent-encoded per segment. A trailing `""` segment yields a trailing
    /// slash. Segments that could change the path structure are refused.
    pub fn endpoint(&self, segments: &[&str]) -> Result<reqwest::Url, PrusaError> {
        let mut url = reqwest::Url::parse(&format!("http://{}/", self.host))
            .map_err(|e| PrusaError::Transport(format!("invalid printer address: {e}")))?;
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|()| PrusaError::Transport("invalid printer address".to_owned()))?;
            path.pop_if_empty();
            for segment in segments {
                if matches!(*segment, "." | "..") || segment.contains(['/', '\\']) {
                    return Err(PrusaError::Decode(format!(
                        "refusing unsafe path segment `{segment}`"
                    )));
                }
                path.push(segment);
            }
        }
        Ok(url)
    }

    /// Sends with digest auth and the request timeout. Non-2xx becomes
    /// [`PrusaError::Status`].
    pub async fn send(
        &self,
        method: Method,
        url: reqwest::Url,
        headers: HeaderMap,
        body: Option<Vec<u8>>,
    ) -> Result<reqwest::Response, PrusaError> {
        let mut request = self
            .http
            .request(method, url)
            .headers(headers)
            .timeout(self.request_timeout);
        if let Some(body) = body {
            request = request.body(body);
        }
        let response = request
            .send_digest_auth((self.username.as_str(), self.password.as_str()))
            .await
            .map_err(map_digest)?;
        self.check(response).await
    }

    /// Turns a non-2xx response into an error carrying the printer's own text.
    async fn check(&self, response: reqwest::Response) -> Result<reqwest::Response, PrusaError> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let body = response.bytes().await.unwrap_or_default();
        Err(PrusaError::Status {
            status,
            detail: error_detail(&body),
        })
    }

    pub async fn get_json<T: DeserializeOwned>(&self, segments: &[&str]) -> Result<T, PrusaError> {
        let url = self.endpoint(segments)?;
        let response = self.send(Method::GET, url, HeaderMap::new(), None).await?;
        response
            .json()
            .await
            .map_err(|e| PrusaError::Decode(e.to_string()))
    }

    /// Like [`Self::get_json`], but a 204 means "nothing there" and gives `None`.
    pub async fn get_optional_json<T: DeserializeOwned>(
        &self,
        segments: &[&str],
    ) -> Result<Option<T>, PrusaError> {
        let url = self.endpoint(segments)?;
        let response = self.send(Method::GET, url, HeaderMap::new(), None).await?;
        if response.status() == StatusCode::NO_CONTENT {
            return Ok(None);
        }
        response
            .json()
            .await
            .map(Some)
            .map_err(|e| PrusaError::Decode(e.to_string()))
    }

    pub async fn version(&self) -> Result<Version, PrusaError> {
        self.get_json(&["api", "version"]).await
    }

    pub async fn info(&self) -> Result<Info, PrusaError> {
        self.get_json(&["api", "v1", "info"]).await
    }

    pub async fn status(&self) -> Result<Status, PrusaError> {
        self.get_json(&["api", "v1", "status"]).await
    }

    pub async fn storage(&self) -> Result<StorageList, PrusaError> {
        self.get_json(&["api", "v1", "storage"]).await
    }

    /// The running job, or `None` when the printer is idle.
    pub async fn job(&self) -> Result<Option<WireJob>, PrusaError> {
        self.get_optional_json(&["api", "v1", "job"]).await
    }

    pub async fn stop_job(&self, id: u64) -> Result<(), PrusaError> {
        self.job_action(Method::DELETE, id, None).await
    }

    pub async fn pause_job(&self, id: u64) -> Result<(), PrusaError> {
        self.job_action(Method::PUT, id, Some("pause")).await
    }

    pub async fn resume_job(&self, id: u64) -> Result<(), PrusaError> {
        self.job_action(Method::PUT, id, Some("resume")).await
    }

    pub async fn continue_job(&self, id: u64) -> Result<(), PrusaError> {
        self.job_action(Method::PUT, id, Some("continue")).await
    }

    /// The running transfer, or `None` when there is none.
    pub async fn transfer(&self) -> Result<Option<WireTransfer>, PrusaError> {
        self.get_optional_json(&["api", "v1", "transfer"]).await
    }

    pub async fn cancel_transfer(&self, id: u64) -> Result<(), PrusaError> {
        let id = id.to_string();
        let url = self.endpoint(&["api", "v1", "transfer", id.as_str()])?;
        self.send(Method::DELETE, url, HeaderMap::new(), None)
            .await?;
        Ok(())
    }

    fn file_segments<'a>(storage: &'a str, segments: &'a [String]) -> Vec<&'a str> {
        let mut all = vec!["api", "v1", "files", storage];
        all.extend(segments.iter().map(String::as_str));
        if segments.is_empty() {
            // The storage root has a trailing slash: `/api/v1/files/usb/`.
            all.push("");
        }
        all
    }

    /// `GET api/v1/files/{storage}/{segments…}`; no segments means the storage root.
    pub async fn file_info(
        &self,
        storage: &str,
        segments: &[String],
    ) -> Result<WireFile, PrusaError> {
        self.get_json(&Self::file_segments(storage, segments)).await
    }

    /// `HEAD` on a file: its `Read-Only` and `Currently-Printed` headers.
    pub async fn file_head(
        &self,
        storage: &str,
        segments: &[String],
    ) -> Result<(bool, bool), PrusaError> {
        let url = self.endpoint(&Self::file_segments(storage, segments))?;
        let response = self.send(Method::HEAD, url, HeaderMap::new(), None).await?;
        let flag = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| matches!(v.trim(), "true" | "?1"))
        };
        Ok((flag("read-only"), flag("currently-printed")))
    }

    /// `POST api/v1/files/{storage}/{path}`: start printing the file.
    pub async fn print_file(&self, storage: &str, segments: &[String]) -> Result<(), PrusaError> {
        let url = self.endpoint(&Self::file_segments(storage, segments))?;
        self.send(Method::POST, url, HeaderMap::new(), None).await?;
        Ok(())
    }

    /// `DELETE api/v1/files/{storage}/{path}` with the `Force` header.
    pub async fn delete_file(
        &self,
        storage: &str,
        segments: &[String],
        force: bool,
    ) -> Result<(), PrusaError> {
        let url = self.endpoint(&Self::file_segments(storage, segments))?;
        let mut headers = HeaderMap::new();
        headers.insert(
            "force",
            HeaderValue::from_static(if force { "?1" } else { "?0" }),
        );
        self.send(Method::DELETE, url, headers, None).await?;
        Ok(())
    }

    /// Uploads a file with `PUT`, streaming `body` (exactly `length` bytes).
    ///
    /// A stream cannot be replayed after a digest challenge, so a bodyless
    /// `HEAD` first collects the challenge; the `PUT` is then signed by hand
    /// and sent once.
    pub async fn upload(
        &self,
        storage: &str,
        segments: &[String],
        length: u64,
        body: reqwest::Body,
        overwrite: bool,
        print_after: bool,
    ) -> Result<(), PrusaError> {
        let url = self.endpoint(&Self::file_segments(storage, segments))?;
        // The encoded path plus query, exactly as it goes on the request line.
        let uri = match url.query() {
            Some(query) => format!("{}?{query}", url.path()),
            None => url.path().to_owned(),
        };

        let probe = self
            .http
            .head(url.clone())
            .timeout(self.upload_timeout)
            .send()
            .await
            .map_err(map_reqwest)?;
        let challenge = (probe.status() == StatusCode::UNAUTHORIZED)
            .then(|| probe.headers().get(reqwest::header::WWW_AUTHENTICATE))
            .flatten()
            .and_then(|v| v.to_str().ok());

        let mut request = self
            .http
            .put(url)
            .timeout(self.upload_timeout)
            .header(reqwest::header::CONTENT_LENGTH, length)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .header("Overwrite", if overwrite { "?1" } else { "?0" })
            .header("Print-After-Upload", if print_after { "?1" } else { "?0" })
            .body(body);
        if let Some(challenge) = challenge {
            let authorization =
                digest::authorization(challenge, &self.username, &self.password, "PUT", &uri)?;
            request = request.header(reqwest::header::AUTHORIZATION, authorization);
        }
        let response = request.send().await.map_err(map_reqwest)?;
        self.check(response).await?;
        Ok(())
    }

    pub async fn cameras(&self) -> Result<Vec<WireCamera>, PrusaError> {
        let list: WireCameraList = self.get_json(&["api", "v1", "cameras"]).await?;
        Ok(list.into_vec())
    }

    pub async fn camera(&self, id: &str) -> Result<WireCameraConfig, PrusaError> {
        self.get_json(&["api", "v1", "cameras", id]).await
    }

    /// A snapshot from the default camera (`None`) or camera `id`. A 204 means
    /// there is no picture yet. The content type falls back to `image/png`.
    pub async fn snapshot(&self, id: Option<&str>) -> Result<Option<(String, Bytes)>, PrusaError> {
        let mut segments = vec!["api", "v1", "cameras"];
        segments.push(id.unwrap_or("snap"));
        if id.is_some() {
            segments.push("snap");
        }
        let url = self.endpoint(&segments)?;
        let response = self.send(Method::GET, url, HeaderMap::new(), None).await?;
        if response.status() == StatusCode::NO_CONTENT {
            return Ok(None);
        }
        Ok(Some(Self::image(response).await?))
    }

    async fn image(response: reqwest::Response) -> Result<(String, Bytes), PrusaError> {
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .filter(|v| v.starts_with("image/"))
            .unwrap_or("image/png")
            .to_owned();
        let bytes = response.bytes().await.map_err(map_reqwest)?;
        Ok((content_type, bytes))
    }

    async fn job_action(
        &self,
        method: Method,
        id: u64,
        action: Option<&str>,
    ) -> Result<(), PrusaError> {
        let id = id.to_string();
        let mut segments = vec!["api", "v1", "job", id.as_str()];
        segments.extend(action);
        let url = self.endpoint(&segments)?;
        self.send(method, url, HeaderMap::new(), None).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use axum::response::IntoResponse;

    use super::*;
    use crate::error::ApiError;

    fn status_of(e: PrusaError) -> StatusCode {
        ApiError::from(e).into_response().status()
    }

    fn up(c: u16) -> PrusaError {
        PrusaError::Status {
            status: StatusCode::from_u16(c).unwrap(),
            detail: None,
        }
    }

    fn client() -> PrusaLinkClient {
        PrusaLinkClient::new(reqwest::Client::new(), "10.0.0.5:8080", "u", "p")
    }

    #[test]
    fn maps_upstream_statuses() {
        assert_eq!(status_of(up(404)), StatusCode::NOT_FOUND);
        assert_eq!(status_of(up(409)), StatusCode::CONFLICT);
        assert_eq!(status_of(up(403)), StatusCode::FORBIDDEN);
        assert_eq!(status_of(up(408)), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(status_of(up(503)), StatusCode::BAD_GATEWAY);
        assert_eq!(status_of(up(401)), StatusCode::BAD_GATEWAY);
        assert_eq!(status_of(up(500)), StatusCode::BAD_GATEWAY);
        assert_eq!(status_of(PrusaError::Timeout), StatusCode::GATEWAY_TIMEOUT);
        // A printer refusal is never a starstar role failure.
        assert!(matches!(ApiError::from(up(403)), ApiError::Printer(_, _)));
    }

    #[test]
    fn endpoint_encodes_per_segment() {
        let c = client();
        assert_eq!(
            c.endpoint(&["api", "v1", "files", "usb", "My Parts", "café #1?%.bgcode"])
                .unwrap()
                .as_str(),
            "http://10.0.0.5:8080/api/v1/files/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode"
        );
        assert_eq!(
            c.endpoint(&["api", "v1", "files", "usb", ""])
                .unwrap()
                .path(),
            "/api/v1/files/usb/"
        );
    }

    #[test]
    fn endpoint_refuses_structural_segments() {
        let c = client();
        assert!(matches!(
            c.endpoint(&["api", "..", "x"]),
            Err(PrusaError::Decode(_))
        ));
        for bad in [".", "a/b", "a\\b"] {
            assert!(c.endpoint(&["api", bad]).is_err(), "{bad}");
        }
    }
}
