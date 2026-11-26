//! A fake starstar API and helpers for driving the real binary against it.
//! Nothing here touches a real printer or leaves 127.0.0.1.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{Request, Response};
use serde::Serialize;
use serde_json::json;
use starstar_cli::credentials::{CredentialStore, StoredToken, server_key};
use starstar_shared::auth::{TokenResponse, WhoAmI};
use starstar_shared::{
    Health, PrinterInfo, PrinterKind, PrinterStatus, PrinterSummary, Role, StorageDevice,
};

pub const TOKEN: &str = "ss_at_test";

#[derive(Clone)]
pub struct Canned {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Canned {
    pub fn json(status: u16, v: impl Serialize) -> Self {
        Self::bytes(status, "application/json", serde_json::to_vec(&v).unwrap())
    }
    pub fn empty(status: u16) -> Self {
        Self {
            status,
            headers: vec![],
            body: vec![],
        }
    }
    pub fn bytes(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: vec![("content-type".into(), content_type.into())],
            body,
        }
    }
    pub fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.into(), v.into()));
        self
    }
}

#[derive(Clone)]
pub struct Recorded {
    pub method: String,
    pub uri: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}

#[derive(Default)]
struct State_ {
    recorded: Vec<Recorded>,
    injected: HashMap<(String, String), Canned>,
    accepted: HashSet<String>,
    live_refresh: HashSet<String>,
    refresh_count: usize,
    counter: usize,
    refresh_delay: Duration,
}

struct Shared {
    url: String,
    state: Mutex<State_>,
}

pub struct FakeApi {
    pub url: String,
    pub addr: SocketAddr,
    shared: Arc<Shared>,
    _rt: tokio::runtime::Runtime,
}

impl FakeApi {
    pub fn start() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{addr}");
        let shared = Arc::new(Shared {
            url: url.clone(),
            state: Mutex::new(State_ {
                accepted: HashSet::from([TOKEN.to_owned()]),
                ..Default::default()
            }),
        });
        let app = Router::new()
            .fallback(handle)
            .layer(DefaultBodyLimit::disable())
            .with_state(shared.clone());
        rt.spawn(async move {
            let l = tokio::net::TcpListener::from_std(listener).unwrap();
            axum::serve(l, app).await.unwrap();
        });
        Self {
            url,
            addr,
            shared,
            _rt: rt,
        }
    }

    /// Overrides the answer for an exact raw path (and query).
    pub fn set(&self, method: &str, uri: &str, canned: Canned) {
        self.shared
            .state
            .lock()
            .unwrap()
            .injected
            .insert((method.to_owned(), uri.to_owned()), canned);
    }

    pub fn accept_token(&self, t: &str) {
        self.shared
            .state
            .lock()
            .unwrap()
            .accepted
            .insert(t.to_owned());
    }

    pub fn forget_token(&self, t: &str) {
        self.shared.state.lock().unwrap().accepted.remove(t);
    }

    pub fn add_refresh_token(&self, t: &str) {
        self.shared
            .state
            .lock()
            .unwrap()
            .live_refresh
            .insert(t.to_owned());
    }

    pub fn set_refresh_delay(&self, d: Duration) {
        self.shared.state.lock().unwrap().refresh_delay = d;
    }

    pub fn refresh_count(&self) -> usize {
        self.shared.state.lock().unwrap().refresh_count
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.shared.state.lock().unwrap().recorded.clone()
    }

    pub fn find(&self, method: &str, uri: &str) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|r| r.method == method && r.uri == uri)
            .collect()
    }

    /// Requests to `/api/*` other than the health check.
    pub fn api_requests(&self) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|r| r.uri.starts_with("/api/") && r.uri != "/api/health")
            .collect()
    }
}

fn to_response(c: Canned) -> Response<Body> {
    let mut b = Response::builder().status(c.status);
    for (k, v) in c.headers {
        b = b.header(k, v);
    }
    b.body(Body::from(c.body)).unwrap()
}

async fn handle(State(sh): State<Arc<Shared>>, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    let body = axum::body::to_bytes(body, usize::MAX)
        .await
        .unwrap_or_default();
    let rec = Recorded {
        method: parts.method.to_string(),
        uri: parts
            .uri
            .path_and_query()
            .map(|p| p.as_str().to_owned())
            .unwrap_or_default(),
        headers: parts
            .headers
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_lowercase(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect(),
        body: body.to_vec(),
    };
    let path = parts.uri.path().to_owned();
    let (canned, delay) = {
        let mut st = sh.state.lock().unwrap();
        st.recorded.push(rec.clone());
        decide(&sh, &mut st, &rec, &path)
    };
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    to_response(canned)
}

fn decide(sh: &Shared, st: &mut State_, rec: &Recorded, path: &str) -> (Canned, Duration) {
    if path.starts_with("/api/") && path != "/api/health" {
        let ok = rec
            .header("authorization")
            .and_then(|v| v.strip_prefix("Bearer "))
            .is_some_and(|t| st.accepted.contains(t));
        if !ok {
            return (
                Canned::json(401, json!({"error": "authentication required"})).header(
                    "www-authenticate",
                    &format!(
                        "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
                        sh.url
                    ),
                ),
                Duration::ZERO,
            );
        }
    }
    if let Some(c) = st.injected.get(&(rec.method.clone(), rec.uri.clone())) {
        return (c.clone(), Duration::ZERO);
    }
    if let Some(r) = oauth(sh, st, rec, path) {
        return r;
    }
    (defaults(&rec.method, path), Duration::ZERO)
}

fn form(rec: &Recorded) -> HashMap<String, String> {
    url::form_urlencoded::parse(&rec.body)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// The OAuth endpoints of the fake.
fn oauth(sh: &Shared, st: &mut State_, rec: &Recorded, path: &str) -> Option<(Canned, Duration)> {
    match (rec.method.as_str(), path) {
        ("POST", "/oauth/token") => {
            let f = form(rec);
            match f.get("grant_type").map(String::as_str) {
                Some("refresh_token") => {
                    let rt = f.get("refresh_token").cloned().unwrap_or_default();
                    if !st.live_refresh.remove(&rt) {
                        return Some((
                            Canned::json(400, json!({"error": "invalid_grant"})),
                            Duration::ZERO,
                        ));
                    }
                    st.refresh_count += 1;
                    st.counter += 1;
                    let n = st.refresh_count;
                    let (at, new_rt) = (format!("ss_at_r{n}"), format!("ss_rt_r{n}"));
                    st.accepted.insert(at.clone());
                    st.live_refresh.insert(new_rt.clone());
                    let body = TokenResponse {
                        access_token: at,
                        token_type: "Bearer".into(),
                        expires_in: 3600,
                        refresh_token: new_rt,
                        scope: "starstar:operator".into(),
                    };
                    Some((Canned::json(200, body), st.refresh_delay))
                }
                _ => None,
            }
        }
        _ => {
            let _ = sh;
            None
        }
    }
}

fn defaults(method: &str, path: &str) -> Canned {
    match (method, path) {
        ("GET", "/api/health") => Canned::json(200, Health::ok()),
        ("GET", "/api/me") => Canned::json(
            200,
            WhoAmI {
                username: "kerry".into(),
                role: Role::Operator,
            },
        ),
        ("GET", "/api/printers") => Canned::json(200, vec![mk4_summary()]),
        ("GET", "/api/printers/mk4") => Canned::json(200, mk4_summary()),
        ("GET", "/api/printers/mk4/info") => Canned::json(
            200,
            PrinterInfo {
                name: "mk4".into(),
                kind: PrinterKind::Mk4,
                firmware: "6.1.2".into(),
                api_version: "2.0.0".into(),
                hostname: Some("prusa-mk4".into()),
                serial: Some("CZPX1234".into()),
                nozzle_diameter: Some(0.4),
                mmu: Some(false),
            },
        ),
        ("GET", "/api/printers/mk4/status") => Canned::json(
            200,
            PrinterStatus {
                name: "mk4".into(),
                state: "PRINTING".into(),
                nozzle_temp: Some(215.0),
                nozzle_target: Some(215.0),
                bed_temp: Some(60.0),
                bed_target: Some(60.0),
            },
        ),
        ("GET", "/api/printers/mk4/storage") => Canned::json(
            200,
            vec![StorageDevice {
                name: Some("usb".into()),
                kind: "USB".into(),
                path: "/usb".into(),
                available: true,
                read_only: Some(false),
                free_space: Some(1_000_000),
                total_space: Some(2_000_000),
            }],
        ),
        _ => Canned::json(404, json!({"error": "not found"})),
    }
}

pub fn mk4_summary() -> PrinterSummary {
    PrinterSummary {
        name: "mk4".into(),
        kind: PrinterKind::Mk4,
        host: "10.0.0.5".into(),
        username: "maker".into(),
    }
}

// ---- stored credentials ----

pub fn stored(fake: &FakeApi, access: &str, refresh: &str, expires_at: i64) -> StoredToken {
    StoredToken {
        access_token: access.into(),
        refresh_token: refresh.into(),
        expires_at,
        scope: "starstar:operator".into(),
        issuer: fake.url.clone(),
        token_endpoint: format!("{}/oauth/token", fake.url),
        revocation_endpoint: Some(format!("{}/oauth/revoke", fake.url)),
    }
}

pub fn credentials_path(home: &Path) -> std::path::PathBuf {
    home.join("config/starstar/credentials.json")
}

pub fn write_credentials(home: &Path, fake: &FakeApi, t: StoredToken) {
    write_credentials_for(home, &fake.url, t);
}

pub fn write_credentials_for(home: &Path, api_url: &str, t: StoredToken) {
    let key = server_key(&api_url.parse().unwrap());
    CredentialStore::at(credentials_path(home))
        .put(&key, t)
        .unwrap();
}

pub fn read_credentials(home: &Path, fake: &FakeApi) -> Option<StoredToken> {
    let key = server_key(&fake.url.parse().unwrap());
    CredentialStore::at(credentials_path(home))
        .get(&key)
        .unwrap()
}

pub fn future() -> i64 {
    starstar_cli::session::now() + 3600
}

// ---- running the binary ----

pub fn cli(fake: &FakeApi, home: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_starstar"));
    c.env("STARSTAR_API_URL", &fake.url)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env_remove("STARSTAR_TOKEN")
        .env("NO_PROXY", "127.0.0.1,localhost")
        .stdin(Stdio::null());
    for var in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        c.env_remove(var);
    }
    c
}

pub struct Out {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Out {
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout is not JSON ({e}): {}\nstderr: {}",
                self.stdout, self.stderr
            )
        })
    }
}

pub fn run(cmd: &mut Command) -> Out {
    let o = cmd.output().unwrap();
    Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

pub fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}
