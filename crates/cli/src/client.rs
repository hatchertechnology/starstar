//! Blocking HTTP client for the starstar API.

use std::error::Error;
use std::time::Duration;

use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use starstar_shared::{
    ErrorBody, Health, PrinterInfo, PrinterStatus, PrinterSummary, RegisterPrinter, StorageDevice,
};

pub type CliResult<T> = Result<T, Box<dyn Error>>;

pub struct ApiClient {
    base_url: String,
    http: Client,
}

impl ApiClient {
    pub fn new(base_url: &str) -> CliResult<Self> {
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            http: Client::builder().timeout(Duration::from_secs(30)).build()?,
        })
    }

    pub fn health(&self) -> CliResult<Health> {
        self.send(self.http.get(self.url("/api/health")))
    }

    pub fn list_printers(&self) -> CliResult<Vec<PrinterSummary>> {
        self.send(self.http.get(self.url("/api/printers")))
    }

    pub fn register(&self, printer: &RegisterPrinter) -> CliResult<PrinterSummary> {
        self.send(self.http.post(self.url("/api/printers")).json(printer))
    }

    pub fn show_printer(&self, name: &str) -> CliResult<PrinterSummary> {
        self.send(self.http.get(self.url(&printer_path(name))))
    }

    pub fn remove_printer(&self, name: &str) -> CliResult<()> {
        self.execute(self.http.delete(self.url(&printer_path(name))))?;
        Ok(())
    }

    pub fn printer_info(&self, name: &str) -> CliResult<PrinterInfo> {
        self.send(
            self.http
                .get(self.url(&format!("{}/info", printer_path(name)))),
        )
    }

    pub fn printer_status(&self, name: &str) -> CliResult<PrinterStatus> {
        self.send(
            self.http
                .get(self.url(&format!("{}/status", printer_path(name)))),
        )
    }

    pub fn printer_storage(&self, name: &str) -> CliResult<Vec<StorageDevice>> {
        self.send(
            self.http
                .get(self.url(&format!("{}/storage", printer_path(name)))),
        )
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    fn send<T: DeserializeOwned>(&self, request: RequestBuilder) -> CliResult<T> {
        Ok(self.execute(request)?.json()?)
    }

    /// Sends the request and turns any non-success response into an error
    /// carrying the API's message.
    fn execute(&self, request: RequestBuilder) -> CliResult<Response> {
        let response = request.send().map_err(|err| {
            format!(
                "could not reach the starstar API at {}: {err}",
                self.base_url
            )
        })?;

        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let message = response
            .json::<ErrorBody>()
            .map(|body| body.error)
            .unwrap_or_else(|_| format!("the API returned HTTP {status}"));
        Err(message.into())
    }
}

fn printer_path(name: &str) -> String {
    format!("/api/printers/{name}")
}
