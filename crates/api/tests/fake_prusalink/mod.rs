//! An in-process fake PrusaLink printer for tests, plus a harness around the
//! starstar router. The fake really verifies HTTP Digest credentials.
//!
//! Tests must never contact a real printer: everything goes through this fake
//! (or `127.0.0.1:1` for an unreachable host).
#![allow(dead_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode, header};
use axum::response::Response;
use md5::{Digest, Md5};
use serde_json::{Value, json};
use starstar_api::AppState;
use starstar_api::config::Config;
use starstar_api::store::StoredPrinter;
use starstar_shared::{PrinterKind, Role};
use tower::ServiceExt;

use crate::common;

pub const USERNAME: &str = "maker";
pub const PASSWORD: &str = "hunter2";
const NONCE: &str = "abc123";
const CHALLENGE: &str = r#"Digest realm="PrusaLink", nonce="abc123", qop="auth", algorithm=MD5"#;

/// A canned response.
#[derive(Clone)]
pub struct Canned {
    pub status: StatusCode,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Canned {
    pub fn json(status: u16, value: Value) -> Self {
        Self::bytes(status, "application/json", value.to_string().into_bytes())
    }

    pub fn empty(status: u16) -> Self {
        Self {
            status: StatusCode::from_u16(status).unwrap(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn bytes(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        Self::empty(status)
            .header("content-type", content_type)
            .with_body(body)
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
        self
    }
}

/// One request the fake received.
#[derive(Clone)]
pub struct Recorded {
    pub method: Method,
    pub uri: String,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
    pub authorized: bool,
}

#[derive(Default)]
struct Inner {
    injected: HashMap<(Method, String), Canned>,
    delay: Duration,
    requests: Vec<Recorded>,
}

#[derive(Clone)]
pub struct FakePrinter {
    pub addr: SocketAddr,
    inner: Arc<Mutex<Inner>>,
}

impl FakePrinter {
    pub async fn start() -> Self {
        let inner = Arc::new(Mutex::new(Inner::default()));
        let app = Router::new()
            .fallback(handle)
            .with_state(Arc::clone(&inner));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { addr, inner }
    }

    /// Injects a response for an exact method and raw path(+query).
    pub fn set(&self, method: Method, uri: &str, canned: Canned) {
        self.inner
            .lock()
            .unwrap()
            .injected
            .insert((method, uri.to_owned()), canned);
    }

    /// Sleeps before every authorized response.
    pub fn set_delay(&self, delay: Duration) {
        self.inner.lock().unwrap().delay = delay;
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.inner.lock().unwrap().requests.clone()
    }

    pub fn authorized(&self, method: Method) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|r| r.authorized && r.method == method)
            .collect()
    }
}

fn md5_hex(input: &str) -> String {
    let digest = Md5::digest(input.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Splits `k=v, k="v, w"` into pairs, honouring quotes.
fn parse_digest(header: &str) -> Option<HashMap<String, String>> {
    let rest = header.strip_prefix("Digest ")?;
    let mut map = HashMap::new();
    let mut key = String::new();
    let mut value = String::new();
    let mut in_key = true;
    let mut quoted = false;
    for c in rest.chars().chain(std::iter::once(',')) {
        match c {
            '"' if !in_key => quoted = !quoted,
            '=' if in_key => in_key = false,
            ',' if !quoted => {
                if !key.trim().is_empty() {
                    map.insert(key.trim().to_owned(), value.trim().to_owned());
                }
                key.clear();
                value.clear();
                in_key = true;
            }
            _ if in_key => key.push(c),
            _ => value.push(c),
        }
    }
    Some(map)
}

fn digest_ok(method: &Method, uri: &str, headers: &HeaderMap) -> bool {
    let Some(parsed) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_digest)
    else {
        return false;
    };
    let get = |k: &str| parsed.get(k).map(String::as_str);
    if get("username") != Some(USERNAME)
        || get("realm") != Some("PrusaLink")
        || get("nonce") != Some(NONCE)
        || get("uri") != Some(uri)
    {
        return false;
    }
    let ha1 = md5_hex(&format!("{USERNAME}:PrusaLink:{PASSWORD}"));
    let ha2 = md5_hex(&format!("{method}:{uri}"));
    let expected = match get("qop") {
        Some(qop) => {
            let (Some(nc), Some(cnonce)) = (get("nc"), get("cnonce")) else {
                return false;
            };
            md5_hex(&format!("{ha1}:{NONCE}:{nc}:{cnonce}:{qop}:{ha2}"))
        }
        None => md5_hex(&format!("{ha1}:{NONCE}:{ha2}")),
    };
    get("response") == Some(expected.as_str())
}

async fn handle(
    axum::extract::State(inner): axum::extract::State<Arc<Mutex<Inner>>>,
    request: Request<Body>,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    let uri = parts
        .uri
        .path_and_query()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_default();
    let authorized = digest_ok(&parts.method, &uri, &parts.headers);

    let (delay, injected) = {
        let mut guard = inner.lock().unwrap();
        guard.requests.push(Recorded {
            method: parts.method.clone(),
            uri: uri.clone(),
            headers: parts.headers.clone(),
            body: body.to_vec(),
            authorized,
        });
        (
            guard.delay,
            guard
                .injected
                .get(&(parts.method.clone(), uri.clone()))
                .cloned(),
        )
    };

    if !authorized {
        return build(Canned::empty(401).header("www-authenticate", CHALLENGE));
    }
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    let canned = injected
        .or_else(|| default_for(&parts.method, &uri))
        .unwrap_or_else(|| Canned::json(404, json!({"title":"Not Found","text":"not found"})));
    build(canned)
}

fn build(canned: Canned) -> Response {
    let mut response = Response::new(Body::from(canned.body));
    *response.status_mut() = canned.status;
    for (name, value) in canned.headers {
        response.headers_mut().append(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(&value).unwrap(),
        );
    }
    response
}

fn job_file() -> Value {
    json!({"name":"CUBE12~1.BGC","display_name":"cube-12mm.bgcode","path":"/usb","display_path":"/usb","size":123456,"m_timestamp":1700000000,"meta":{"estimated_print_time":3000,"filament_type":"PLA","layer_height":0.2,"filament used [g]":3.1}})
}

fn png() -> Canned {
    Canned::bytes(200, "image/png", b"\x89PNG\r\n\x1a\nfake".to_vec())
}

fn default_for(method: &Method, uri: &str) -> Option<Canned> {
    let m = method.as_str();
    let canned = match (m, uri) {
        ("GET", "/api/version") => Canned::json(
            200,
            json!({"api":"2.0.0","server":"2.1.2","firmware":"8.0.2+17092","printer":"1.4.0","text":"PrusaLink","version":"2.1.2"}),
        ),
        ("GET", "/api/v1/info") => Canned::json(
            200,
            json!({"hostname":"prusa-mk4","serial":"10589-3742441631719910","nozzle_diameter":0.4,"mmu":false}),
        ),
        ("GET", "/api/v1/status") => Canned::json(
            200,
            json!({"printer":{"state":"PRINTING","temp_nozzle":215.0,"target_nozzle":215.0,"temp_bed":60.0,"target_bed":60.0},"job":{"id":42,"progress":37.5},"transfer":{"id":7,"time_transferring":12}}),
        ),
        ("GET", "/api/v1/storage") => Canned::json(
            200,
            json!({"storage_list":[{"path":"/usb/","name":"usb","type":"USB","read_only":false,"available":true}]}),
        ),
        ("GET", "/api/v1/job") => Canned::json(
            200,
            json!({"id":42,"state":"PRINTING","progress":37.5,"time_remaining":1800,"time_printing":1200,"inaccurate_estimates":false,"file":job_file()}),
        ),
        ("DELETE", "/api/v1/job/42")
        | ("PUT", "/api/v1/job/42/pause" | "/api/v1/job/42/resume" | "/api/v1/job/42/continue") => {
            Canned::empty(204)
        }
        ("GET", "/api/v1/transfer") => Canned::json(
            200,
            json!({"type":"FROM_CLIENT","display_name":"big.bgcode","path":"/usb","size":2048,"progress":0.5,"transferred":1024,"time_remaining":10,"time_transferring":12,"to_print":false}),
        ),
        ("DELETE", "/api/v1/transfer/7") => Canned::empty(204),
        ("GET", "/api/v1/files/usb/") => Canned::json(
            200,
            json!({"name":"usb","type":"FOLDER","read_only":false,"m_timestamp":1700000000,"children":[{"name":"CUBE12~1.BGC","display_name":"cube-12mm.bgcode","type":"PRINT_FILE","read_only":false,"size":123456,"m_timestamp":1700000000},{"name":"parts","type":"FOLDER","read_only":false,"m_timestamp":1700000000}]}),
        ),
        ("GET", "/api/v1/files/usb/cube-12mm.bgcode") => Canned::json(
            200,
            json!({"name":"cube-12mm.bgcode","type":"PRINT_FILE","read_only":false,"size":123456,"m_timestamp":1700000000,"meta":{"estimated_print_time":3000,"filament_type":"PLA","layer_height":0.2,"nozzle_diameter":0.4,"printer_model":"MK4","temperature":215,"bed_temperature":60,"filament used [g]":3.1,"filament used [mm]":1040.5},"refs":{"thumbnail":"/thumb/l/usb/CUBE12~1.BGC","download":"/usb/CUBE12~1.BGC"}}),
        ),
        ("HEAD", "/api/v1/files/usb/cube-12mm.bgcode") => Canned::empty(200)
            .header("Read-Only", "false")
            .header("Currently-Printed", "true"),
        ("GET", "/api/v1/cameras") => Canned::json(
            200,
            json!([{"camera_id":"cam1","config":{"name":"Buddy3D","driver":"V4L2","path":"/dev/video0","resolution":"1280x720"},"connected":true,"detected":true,"stored":true,"linked":false}]),
        ),
        ("PUT", "/api/v1/cameras") => Canned::empty(200),
        ("GET", "/api/v1/cameras/cam1") => Canned::json(
            200,
            json!({"name":"Buddy3D","trigger_scheme":"THIRTY_SEC","available_resolutions":[{"width":1280,"height":720}],"resolution":{"width":1280,"height":720},"focus":0.5,"capabilities":["TRIGGER_SCHEME","RESOLUTION"]}),
        ),
        ("POST" | "DELETE", "/api/v1/cameras/cam1")
        | ("PATCH" | "DELETE", "/api/v1/cameras/cam1/config")
        | ("POST" | "DELETE", "/api/v1/cameras/cam1/connection") => Canned::empty(200),
        ("GET", "/thumb/l/usb/CUBE12~1.BGC") => png(),
        ("GET", "/api/v1/cameras/snap" | "/api/v1/cameras/cam1/snap")
        | ("POST", "/api/v1/cameras/cam1/snap") => png(),
        ("GET", "/api/v1/update/prusalink") => {
            Canned::json(200, json!({"new_version":"0.8.1"})).header("Update-Available", "true")
        }
        ("POST", "/api/v1/update/prusalink") => Canned::empty(204),
        (_, _) => {
            if let Some(rest) = uri.strip_prefix("/api/v1/files/")
                && !rest.is_empty()
            {
                return match m {
                    "PUT" => Some(Canned::empty(201)),
                    "POST" | "DELETE" => Some(Canned::empty(204)),
                    "HEAD" => Some(
                        Canned::empty(200)
                            .header("Read-Only", "false")
                            .header("Currently-Printed", "false"),
                    ),
                    _ => None,
                };
            }
            return None;
        }
    };
    Some(canned)
}

pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl TestResponse {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("not JSON ({e}): {}", self.text()))
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// The starstar router wired to a fake printer registered as `mk4`.
pub struct TestApi {
    pub app: Router,
    pub state: AppState,
    pub fake: FakePrinter,
}

impl TestApi {
    pub async fn start() -> Self {
        Self::start_with(Config::default()).await
    }

    pub async fn start_with(config: Config) -> Self {
        let state = AppState {
            config: Arc::new(config),
            ..common::test_state().await
        };
        let fake = FakePrinter::start().await;
        let api = Self {
            app: starstar_api::router(state.clone()),
            state,
            fake,
        };
        api.add_printer("mk4", &api.fake.addr.to_string(), PASSWORD)
            .await;
        api
    }

    /// Registers a printer directly in the store, skipping the reachability check.
    pub async fn add_printer(&self, name: &str, host: &str, password: &str) {
        self.state
            .store
            .insert(StoredPrinter {
                name: name.to_owned(),
                kind: PrinterKind::Mk4,
                host: host.to_owned(),
                username: USERNAME.to_owned(),
                password: password.to_owned(),
            })
            .await
            .unwrap();
    }

    pub async fn call(
        &self,
        role: Role,
        method: Method,
        uri: &str,
        headers: &[(&str, &str)],
        body: Body,
    ) -> TestResponse {
        let mut builder = Request::builder().method(method).uri(uri);
        for (k, v) in headers {
            builder = builder.header(*k, *v);
        }
        let builder = common::authorize(builder, &self.state, role).await;
        self.dispatch(builder.body(body).unwrap()).await
    }

    pub async fn call_anonymous(
        &self,
        method: Method,
        uri: &str,
        headers: &[(&str, &str)],
        body: Body,
    ) -> TestResponse {
        let mut builder = Request::builder().method(method).uri(uri);
        for (k, v) in headers {
            builder = builder.header(*k, *v);
        }
        self.dispatch(builder.body(body).unwrap()).await
    }

    pub async fn get(&self, role: Role, uri: &str) -> TestResponse {
        self.call(role, Method::GET, uri, &[], Body::empty()).await
    }

    pub async fn send_json(
        &self,
        role: Role,
        method: Method,
        uri: &str,
        json: Value,
    ) -> TestResponse {
        self.call(
            role,
            method,
            uri,
            &[("content-type", "application/json")],
            Body::from(json.to_string()),
        )
        .await
    }

    async fn dispatch(&self, request: Request<Body>) -> TestResponse {
        let response = self.app.clone().oneshot(request).await.unwrap();
        let (parts, body) = response.into_parts();
        TestResponse {
            status: parts.status,
            headers: parts.headers,
            body: axum::body::to_bytes(body, usize::MAX).await.unwrap(),
        }
    }
}
