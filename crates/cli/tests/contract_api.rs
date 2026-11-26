//! The CLI against the real starstar API router: login, refresh, revocation
//! and the existence of every route the CLI calls. No printer is involved.

use std::io;
use std::net::SocketAddr;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use serde_json::json;
use starstar_api::auth::AuthStore;
use starstar_api::auth::clock::Clock;
use starstar_api::auth::password::Hasher;
use starstar_api::config::Config;
use starstar_api::prusalink::http_client;
use starstar_api::store::{Db, PrinterStore};
use starstar_api::{AppState, router};
use starstar_cli::commands::account::{LoginMethod, login_with};
use starstar_cli::credentials::{CredentialStore, StoredToken, server_key};
use starstar_cli::endpoints::{self, Endpoint, JobAction, RemotePath};
use starstar_cli::output::Output;
use starstar_shared::Role;

const ADMIN: &str = "kerry";
const ADMIN_PASSWORD: &str = "pw-kerry-123";

struct RealApi {
    url: String,
    state: AppState,
    _rt: tokio::runtime::Runtime,
}

impl RealApi {
    /// `with_admin` creates the admin `kerry`; without it the API is waiting for setup.
    fn start(with_admin: bool) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}");
        // Built like `crates/api/tests/common::test_state()`, but with the
        // real clock, because tokens must expire in real time here.
        let db = Db::open_in_memory().unwrap();
        let state = AppState {
            store: Arc::new(PrinterStore::from_db(db.clone())),
            auth: Arc::new(AuthStore::new(db, Clock::system(), Hasher::fast_for_tests()).unwrap()),
            http: http_client().unwrap(),
            config: Arc::new(Config {
                public_url: url.clone(),
                ..Config::default()
            }),
        };
        if with_admin {
            let hash = state.auth.hasher().hash(ADMIN_PASSWORD);
            state.auth.create_user(ADMIN, &hash, Role::Admin).unwrap();
        }
        let app = router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
        rt.spawn(async move {
            let l = tokio::net::TcpListener::from_std(listener).unwrap();
            axum::serve(l, app).await.unwrap();
        });
        Self {
            url,
            state,
            _rt: rt,
        }
    }
}

fn http() -> Client {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .unwrap()
}

/// A signed-in browser session: cookies and the CSRF token.
struct Session {
    cookie: String,
    csrf: String,
}

fn sign_in(api_url: &str) -> Session {
    let r = http()
        .post(format!("{api_url}/api/session"))
        .json(&json!({"username": ADMIN, "password": ADMIN_PASSWORD}))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let cookie = r
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let csrf = r.json::<serde_json::Value>().unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();
    Session { cookie, csrf }
}

/// Runs the consent flow for `authorize_url` and returns the redirect target.
fn consent(api_url: &str, role: Role, authorize_url: &str) -> String {
    let session = sign_in(api_url);
    let c = http();
    let r = c
        .get(authorize_url)
        .header("cookie", &session.cookie)
        .send()
        .unwrap();
    assert_eq!(r.status(), 303, "authorize");
    let loc = r.headers()["location"].to_str().unwrap().to_owned();
    let id = loc
        .strip_prefix("/consent?request=")
        .expect(&loc)
        .to_owned();
    let r = c
        .post(format!("{api_url}/oauth/authorize/decision"))
        .header("cookie", &session.cookie)
        .header("x-csrf-token", &session.csrf)
        .form(&[
            ("request", id.as_str()),
            ("decision", "allow"),
            ("role", role.as_str()),
            (
                "allow_admin",
                if role == Role::Admin { "true" } else { "false" },
            ),
        ])
        .send()
        .unwrap();
    assert_eq!(
        r.status(),
        303,
        "decision: {}",
        r.text().unwrap_or_default()
    );
    r.headers()["location"].to_str().unwrap().to_owned()
}

/// The "browser" for the loopback flow: signs in, approves, follows the redirect.
fn real_browser(api_url: String, role: Role) -> impl Fn(&str) -> io::Result<()> {
    move |url: &str| {
        let (api_url, url) = (api_url.clone(), url.to_owned());
        std::thread::spawn(move || {
            let target = consent(&api_url, role, &url);
            http().get(target).send().unwrap();
        });
        Ok(())
    }
}

fn store(home: &std::path::Path) -> CredentialStore {
    CredentialStore::at(home.join("config/starstar/credentials.json"))
}

fn login(api: &RealApi, home: &std::path::Path, role: Role) {
    login_with(
        &api.url.parse().unwrap(),
        store(home),
        role,
        LoginMethod::Loopback {
            open: &real_browser(api.url.clone(), role),
            timeout: Duration::from_secs(10),
        },
        &Output { json: true },
    )
    .unwrap();
}

fn stored(api: &RealApi, home: &std::path::Path) -> StoredToken {
    store(home)
        .get(&server_key(&api.url.parse().unwrap()))
        .unwrap()
        .unwrap()
}

fn bin(api: &RealApi, home: &std::path::Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_starstar"));
    c.env("STARSTAR_API_URL", &api.url)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env_remove("STARSTAR_TOKEN")
        .env("NO_PROXY", "127.0.0.1,localhost")
        .stdin(Stdio::null());
    c
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(c: &mut Command) -> Out {
    let o = c.output().unwrap();
    Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

#[test]
fn real_loopback_login_whoami_refresh_logout() {
    let api = RealApi::start(true);
    let home = tempfile::tempdir().unwrap();
    login(&api, home.path(), Role::Operator);

    let who = run(bin(&api, home.path()).args(["--json", "whoami"]));
    assert_eq!(who.code, 0, "{}", who.stderr);
    let v: serde_json::Value = serde_json::from_str(&who.stdout).unwrap();
    assert_eq!(
        (v["username"].as_str(), v["role"].as_str()),
        (Some("kerry"), Some("operator"))
    );

    // An expired access token is refreshed, and the refresh token rotates.
    let mut t = stored(&api, home.path());
    let old_refresh = t.refresh_token.clone();
    t.expires_at = 0;
    store(home.path())
        .put(&server_key(&api.url.parse().unwrap()), t)
        .unwrap();
    let list = run(bin(&api, home.path()).args(["--json", "printer", "list"]));
    assert_eq!(list.code, 0, "{}", list.stderr);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&list.stdout).unwrap(),
        json!([])
    );
    assert_ne!(stored(&api, home.path()).refresh_token, old_refresh);

    // An operator token cannot manage users.
    let users = run(bin(&api, home.path()).args(["user", "list"]));
    assert_eq!(users.code, 4, "{}", users.stderr);

    let access = stored(&api, home.path()).access_token;
    let out = run(bin(&api, home.path()).arg("logout"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    // Revoking the refresh token ended the whole grant.
    let who = run(bin(&api, home.path())
        .env("STARSTAR_TOKEN", access)
        .arg("whoami"));
    assert_eq!(who.code, 3, "{}", who.stderr);
}

#[test]
fn real_copy_code_login() {
    let api = RealApi::start(true);
    let home = tempfile::tempdir().unwrap();
    let url = api.url.clone();
    let mut prompt = move |authorize: &str| -> io::Result<String> {
        let target = consent(&url, Role::Operator, authorize);
        assert!(
            target.starts_with(&format!("{url}/oauth/code?")),
            "{target}"
        );
        let session = sign_in(&url);
        let body = http()
            .get(&target)
            .header("cookie", session.cookie)
            .send()
            .unwrap()
            .text()
            .unwrap();
        let start = body.find("<code id=\"code\">").expect(&body) + 16;
        let end = body[start..].find("</code>").unwrap() + start;
        Ok(body[start..end].to_owned())
    };
    login_with(
        &api.url.parse().unwrap(),
        store(home.path()),
        Role::Operator,
        LoginMethod::CopyCode {
            prompt: &mut prompt,
        },
        &Output { json: true },
    )
    .unwrap();
    let who = run(bin(&api, home.path()).arg("whoami"));
    assert_eq!(who.code, 0, "{}", who.stderr);
    assert!(who.stdout.contains("kerry"));
}

#[test]
fn real_setup_then_login() {
    use std::io::Write;
    let api = RealApi::start(false);
    api.state.auth.set_setup_code(Some("setup-code-1".into()));
    let home = tempfile::tempdir().unwrap();
    let mut child = bin(&api, home.path())
        .args([
            "setup",
            "--username",
            "boss",
            "--setup-code-stdin",
            "--password-stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "setup-code-1\nboss-password-1\n"
    )
    .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(api.state.auth.user_by_name("boss").unwrap().is_some());
    // Setup is closed now.
    assert_eq!(api.state.auth.user_count().unwrap(), 1);
}

#[test]
fn upload_retries_after_refresh() {
    let api = RealApi::start(true);
    let home = tempfile::tempdir().unwrap();
    login(&api, home.path(), Role::Operator);
    // A fresh-looking but unknown access token: the API answers 401.
    let mut t = stored(&api, home.path());
    let old_refresh = t.refresh_token.clone();
    t.access_token = "ss_at_not_a_real_token".into();
    store(home.path())
        .put(&server_key(&api.url.parse().unwrap()), t)
        .unwrap();
    let file = home.path().join("a.bgcode");
    std::fs::write(&file, vec![7u8; 100_000]).unwrap();
    let out = run(bin(&api, home.path())
        .args(["files", "upload", "ghost", "usb"])
        .arg(&file));
    // The retry reached the handler with a fresh token: the printer is unknown.
    assert_eq!(out.code, 1, "{}", out.stderr);
    assert!(out.stderr.contains("ghost"), "{}", out.stderr);
    assert_ne!(stored(&api, home.path()).refresh_token, old_refresh);
}

// ---- every route the CLI calls exists ----

enum Expect {
    Ok200,
    /// A handler answered 404 with an error naming the missing thing.
    Ghost,
    /// A handler answered 404 with a JSON error that does not name the thing.
    NotFoundJson,
    /// HEAD has no body: any 404 will do.
    NotFound,
    /// The body was rejected before the handler ran.
    Status(u16),
}

fn every_printer_endpoint() -> Vec<(Endpoint, Vec<u8>, Expect)> {
    let p = "ghost";
    let path = RemotePath::parse("usb/a.bgcode", false).unwrap().0;
    let root = RemotePath::parse("usb", true).unwrap().0;
    let none = Vec::new;
    let empty_obj = || b"{}".to_vec();
    vec![
        (endpoints::printers(), none(), Expect::Ok200),
        (
            endpoints::register_printer(),
            empty_obj(),
            Expect::Status(422),
        ),
        (endpoints::printer(p), none(), Expect::Ghost),
        (endpoints::remove_printer(p), none(), Expect::Ghost),
        (endpoints::printer_info(p), none(), Expect::Ghost),
        (endpoints::printer_status(p), none(), Expect::Ghost),
        (endpoints::printer_storage(p), none(), Expect::Ghost),
        (endpoints::job(p), none(), Expect::Ghost),
        (endpoints::stop_job(p, 1), none(), Expect::Ghost),
        (
            endpoints::job_action(p, 1, JobAction::Pause),
            none(),
            Expect::Ghost,
        ),
        (
            endpoints::job_action(p, 1, JobAction::Resume),
            none(),
            Expect::Ghost,
        ),
        (
            endpoints::job_action(p, 1, JobAction::Continue),
            none(),
            Expect::Ghost,
        ),
        (endpoints::transfer(p), none(), Expect::Ghost),
        (endpoints::cancel_transfer(p, 1), none(), Expect::Ghost),
        (
            endpoints::update_check(p, "prusalink"),
            none(),
            Expect::Ghost,
        ),
        (
            endpoints::update_apply(p, "prusalink"),
            none(),
            Expect::Ghost,
        ),
        (endpoints::file_get(p, &root), none(), Expect::Ghost),
        (endpoints::file_get(p, &path), none(), Expect::Ghost),
        (endpoints::file_head(p, &path), none(), Expect::NotFound),
        (endpoints::file_print(p, &path), none(), Expect::Ghost),
        (
            endpoints::file_delete(p, &path, true),
            none(),
            Expect::Ghost,
        ),
        (
            endpoints::file_upload(p, &path, true, false),
            none(),
            Expect::Ghost,
        ),
        (endpoints::cameras(p), none(), Expect::Ghost),
        (endpoints::camera(p, "cam1").unwrap(), none(), Expect::Ghost),
        (endpoints::snapshot(p, None).unwrap(), none(), Expect::Ghost),
        (
            endpoints::snapshot(p, Some("cam1")).unwrap(),
            none(),
            Expect::Ghost,
        ),
        (
            endpoints::take_snapshot(p, "cam1").unwrap(),
            none(),
            Expect::Ghost,
        ),
        (endpoints::camera_order(p), b"[]".to_vec(), Expect::Ghost),
        (
            endpoints::register_camera(p, "cam1").unwrap(),
            empty_obj(),
            Expect::Ghost,
        ),
        (
            endpoints::remove_camera(p, "cam1").unwrap(),
            none(),
            Expect::Ghost,
        ),
        (
            endpoints::camera_config(p, "cam1").unwrap(),
            empty_obj(),
            Expect::Ghost,
        ),
        (
            endpoints::reset_camera(p, "cam1").unwrap(),
            none(),
            Expect::Ghost,
        ),
        (
            endpoints::connect_camera(p, "cam1").unwrap(),
            none(),
            Expect::Ghost,
        ),
        (
            endpoints::disconnect_camera(p, "cam1").unwrap(),
            none(),
            Expect::Ghost,
        ),
    ]
}

fn account_endpoints() -> Vec<(Endpoint, Vec<u8>, Expect)> {
    vec![
        (endpoints::me(), Vec::new(), Expect::Ok200),
        (endpoints::users(), Vec::new(), Expect::Ok200),
        (endpoints::add_user(), b"{}".to_vec(), Expect::Status(422)),
        (
            endpoints::update_user("ghost"),
            br#"{"disabled":false}"#.to_vec(),
            Expect::Ghost,
        ),
        (endpoints::remove_user("ghost"), Vec::new(), Expect::Ghost),
        (
            endpoints::my_password(),
            b"{}".to_vec(),
            Expect::Status(422),
        ),
        (endpoints::apps(false), Vec::new(), Expect::Ok200),
        (endpoints::apps(true), Vec::new(), Expect::Ok200),
        (
            endpoints::revoke_app("ss_gr_ghost"),
            Vec::new(),
            Expect::NotFoundJson,
        ),
    ]
}

#[test]
fn real_api_has_every_cli_route() {
    let api = RealApi::start(true);
    let home = tempfile::tempdir().unwrap();
    login(&api, home.path(), Role::Admin);
    let token = stored(&api, home.path()).access_token;
    let base: url::Url = api.url.parse().unwrap();
    let client = http();

    let printer_rows = every_printer_endpoint();
    assert_eq!(printer_rows.len(), 34, "the API has 34 printer routes");
    for (ep, body, expect) in printer_rows.into_iter().chain(account_endpoints()) {
        let mut rb = client
            .request(ep.method.clone(), ep.url(&base))
            .bearer_auth(&token);
        if ep.method == reqwest::Method::PUT && ep.segments.iter().any(|s| s == "files") {
            rb = rb.header(CONTENT_LENGTH, 0);
        } else if !body.is_empty() {
            rb = rb.header(CONTENT_TYPE, "application/json").body(body);
        }
        let r = rb.send().unwrap();
        let status = r.status().as_u16();
        let text = r.text().unwrap_or_default();
        let what = format!("{} {}: {status} {text}", ep.method, ep.url(&base));
        match expect {
            Expect::Ok200 => assert_eq!(status, 200, "{what}"),
            Expect::NotFound => assert_eq!(status, 404, "{what}"),
            Expect::Status(want) => assert_eq!(status, want, "{what}"),
            Expect::NotFoundJson => {
                assert_eq!(status, 404, "{what}");
                let v: serde_json::Value = serde_json::from_str(&text).expect(&what);
                assert!(v["error"].is_string(), "{what}");
            }
            Expect::Ghost => {
                assert_eq!(status, 404, "{what}");
                let v: serde_json::Value = serde_json::from_str(&text).expect(&what);
                assert!(
                    v["error"].as_str().is_some_and(|e| e.contains("ghost")),
                    "{what}"
                );
            }
        }
    }
}
