//! Client for the PrusaLink REST API that Prusa printers expose on the LAN.
//!
//! The endpoints used here are described in `docs/prusalink-openapi.yaml`.
//! Every endpoint requires HTTP Digest authentication.

use std::time::Duration;

use diqwest::WithDigestAuth;
use reqwest::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;

#[derive(Debug, thiserror::Error)]
pub enum PrusaError {
    #[error("could not reach printer: {0}")]
    Transport(String),
    #[error("printer responded with HTTP {0}")]
    Http(StatusCode),
    #[error("unexpected response from printer: {0}")]
    Decode(String),
}

/// Builds the HTTP client shared by all printer requests.
pub fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
}

/// `GET /api/version`
#[derive(Debug, Deserialize)]
pub struct Version {
    pub api: String,
    pub firmware: String,
}

/// `GET /api/v1/info`
#[derive(Debug, Deserialize)]
pub struct Info {
    pub hostname: Option<String>,
    pub serial: Option<String>,
    pub nozzle_diameter: Option<f64>,
    pub mmu: Option<bool>,
}

/// `GET /api/v1/status`
#[derive(Debug, Deserialize)]
pub struct Status {
    pub printer: StatusPrinter,
}

#[derive(Debug, Deserialize)]
pub struct StatusPrinter {
    pub state: String,
    pub temp_nozzle: Option<f64>,
    pub target_nozzle: Option<f64>,
    pub temp_bed: Option<f64>,
    pub target_bed: Option<f64>,
}

/// `GET /api/v1/storage`
#[derive(Debug, Deserialize)]
pub struct StorageList {
    pub storage_list: Vec<Storage>,
}

#[derive(Debug, Deserialize)]
pub struct Storage {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    pub path: String,
    pub available: bool,
    pub read_only: Option<bool>,
    pub free_space: Option<u64>,
    pub total_space: Option<u64>,
}

/// A client bound to one printer's address and credentials.
pub struct PrusaLinkClient {
    http: reqwest::Client,
    base_url: String,
    username: String,
    password: String,
}

impl PrusaLinkClient {
    pub fn new(http: reqwest::Client, host: &str, username: &str, password: &str) -> Self {
        Self {
            http,
            base_url: format!("http://{host}"),
            username: username.to_owned(),
            password: password.to_owned(),
        }
    }

    pub async fn version(&self) -> Result<Version, PrusaError> {
        self.get("/api/version").await
    }

    pub async fn info(&self) -> Result<Info, PrusaError> {
        self.get("/api/v1/info").await
    }

    pub async fn status(&self) -> Result<Status, PrusaError> {
        self.get("/api/v1/status").await
    }

    pub async fn storage(&self) -> Result<StorageList, PrusaError> {
        self.get("/api/v1/storage").await
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, PrusaError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .http
            .get(url)
            .send_digest_auth((self.username.as_str(), self.password.as_str()))
            .await
            .map_err(|e| PrusaError::Transport(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            return Err(PrusaError::Http(status));
        }

        response
            .json()
            .await
            .map_err(|e| PrusaError::Decode(e.to_string()))
    }
}
