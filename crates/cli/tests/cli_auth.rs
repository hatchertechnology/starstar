mod support;

use std::time::Duration;

use starstar_cli::client::{ApiClient, build_http};
use starstar_cli::commands::account::{LoginMethod, login_with};
use starstar_cli::credentials::CredentialStore;
use starstar_cli::endpoints;
use starstar_cli::error::CliError;
use starstar_cli::login::{login_copy_code, login_loopback};
use starstar_cli::output::Output;
use starstar_cli::session::{PlainHttpWarner, TokenSource};
use starstar_shared::Role;
use support::*;

fn with_token(fake: &FakeApi, home: &std::path::Path, token: &str) -> std::process::Command {
    let mut c = cli(fake, home);
    c.env("STARSTAR_TOKEN", token);
    c
}

#[test]
fn not_signed_in_exits_3() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let out = run(cli(&fake, home.path()).args(["printer", "list"]));
    assert_eq!(out.code, 3);
    assert!(
        out.stderr.contains("run `starstar login`"),
        "{}",
        out.stderr
    );
    assert!(fake.api_requests().is_empty());
}

#[test]
fn env_token_overrides_stored_and_never_refreshes() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    write_credentials(
        home.path(),
        &fake,
        stored(&fake, "ss_at_old", "ss_rt_old", 0),
    );
    let before = std::fs::read(credentials_path(home.path())).unwrap();
    let out = run(with_token(&fake, home.path(), TOKEN).args(["printer", "list"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    let reqs = fake.find("GET", "/api/printers");
    assert_eq!(reqs[0].header("authorization"), Some("Bearer ss_at_test"));
    assert_eq!(fake.refresh_count(), 0);
    assert_eq!(
        std::fs::read(credentials_path(home.path())).unwrap(),
        before
    );
}

#[test]
fn rejected_env_token_exits_3() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let out = run(with_token(&fake, home.path(), "ss_at_nope").args(["printer", "list"]));
    assert_eq!(out.code, 3);
    assert!(
        out.stderr.contains("STARSTAR_TOKEN was rejected"),
        "{}",
        out.stderr
    );
    assert_eq!(fake.refresh_count(), 0);
}

#[test]
fn expired_stored_token_is_refreshed_and_saved() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    fake.add_refresh_token("ss_rt_1");
    write_credentials(home.path(), &fake, stored(&fake, "ss_at_x", "ss_rt_1", 0));
    let out = run(cli(&fake, home.path()).args(["printer", "list"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    let reqs = fake.find("GET", "/api/printers");
    assert_eq!(reqs[0].header("authorization"), Some("Bearer ss_at_r1"));
    let saved = read_credentials(home.path(), &fake).unwrap();
    assert_eq!(saved.refresh_token, "ss_rt_r1");
    assert!(saved.expires_at > starstar_cli::session::now());
    assert!(!out.stdout.contains("ss_") && !out.stderr.contains("ss_"));
}

#[test]
fn rejected_fresh_token_triggers_one_refresh_and_retry() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    fake.add_refresh_token("ss_rt_1");
    write_credentials(
        home.path(),
        &fake,
        stored(&fake, "ss_at_revoked", "ss_rt_1", future()),
    );
    let out = run(cli(&fake, home.path()).args(["printer", "list"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    let reqs = fake.find("GET", "/api/printers");
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[1].header("authorization"), Some("Bearer ss_at_r1"));
    assert_eq!(fake.refresh_count(), 1);
}

#[test]
fn refresh_rejected_removes_entry_and_exits_3() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    write_credentials(
        home.path(),
        &fake,
        stored(&fake, "ss_at_x", "ss_rt_dead", 0),
    );
    write_credentials_for(
        home.path(),
        "http://other.example",
        stored(&fake, "ss_at_o", "ss_rt_o", future()),
    );
    let out = run(cli(&fake, home.path()).args(["printer", "list"]));
    assert_eq!(out.code, 3);
    assert!(
        out.stderr.contains("expired or was revoked"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("starstar login"));
    assert!(read_credentials(home.path(), &fake).is_none());
    let other = starstar_cli::credentials::CredentialStore::at(credentials_path(home.path()))
        .get("http://other.example")
        .unwrap();
    assert!(other.is_some());
}

#[test]
fn other_oauth_errors_keep_the_entry() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    fake.set(
        "POST",
        "/oauth/token",
        Canned::json(400, serde_json::json!({"error": "invalid_request"})),
    );
    write_credentials(home.path(), &fake, stored(&fake, "ss_at_x", "ss_rt_1", 0));
    let out = run(cli(&fake, home.path()).args(["printer", "list"]));
    assert_eq!(out.code, 1, "{}", out.stderr);
    assert!(read_credentials(home.path(), &fake).is_some());
}

#[test]
fn concurrent_processes_refresh_once() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    fake.add_refresh_token("ss_rt_1");
    fake.set_refresh_delay(Duration::from_millis(500));
    write_credentials(home.path(), &fake, stored(&fake, "ss_at_x", "ss_rt_1", 0));
    let spawn = || {
        cli(&fake, home.path())
            .args(["printer", "list"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    };
    let (a, b) = (spawn(), spawn());
    let (a, b) = (a.wait_with_output().unwrap(), b.wait_with_output().unwrap());
    assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    assert_eq!(fake.refresh_count(), 1);
    let reqs = fake.find("GET", "/api/printers");
    assert_eq!(reqs.len(), 2);
    assert!(
        reqs.iter()
            .all(|r| r.header("authorization") == Some("Bearer ss_at_r1"))
    );
}

#[test]
fn no_plain_http_warning_on_loopback() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let out = run(with_token(&fake, home.path(), TOKEN).args(["printer", "list"]));
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

struct Buf(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
impl std::io::Write for Buf {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn lan_client(
    fake: &FakeApi,
    with_token: bool,
) -> (ApiClient, std::sync::Arc<std::sync::Mutex<Vec<u8>>>) {
    let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let warner: &'static PlainHttpWarner =
        Box::leak(Box::new(PlainHttpWarner::new(Box::new(Buf(buf.clone())))));
    let http = reqwest::blocking::Client::builder()
        .resolve("printer.lan", fake.addr)
        .no_proxy()
        .build()
        .unwrap();
    let base: url::Url = format!("http://printer.lan:{}", fake.addr.port())
        .parse()
        .unwrap();
    let tokens = with_token.then(|| TokenSource::env(base.clone(), TOKEN.to_owned()));
    (ApiClient::new(base, tokens, http, warner), buf)
}

#[test]
fn plain_http_warning_once_per_process() {
    let fake = FakeApi::start();
    let (client, buf) = lan_client(&fake, true);
    let _: serde_json::Value = client.get_json(&endpoints::printers()).unwrap();
    let _: serde_json::Value = client.get_json(&endpoints::printers()).unwrap();
    let text = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("printer.lan"));
}

#[test]
fn status_over_lan_http_does_not_warn() {
    let fake = FakeApi::start();
    let (client, buf) = lan_client(&fake, false);
    let _: serde_json::Value = client.get_json(&endpoints::health()).unwrap();
    assert!(buf.lock().unwrap().is_empty());
}

// ---- loopback login ----

fn login(
    fake: &FakeApi,
    timeout: Duration,
    open: &dyn Fn(&str) -> std::io::Result<()>,
) -> Result<starstar_cli::credentials::StoredToken, CliError> {
    let api: url::Url = fake.url.parse().unwrap();
    login_loopback(
        &build_http().unwrap(),
        &api,
        Role::Operator,
        timeout,
        open,
        &mut Vec::new(),
    )
}

#[test]
fn loopback_login_end_to_end() {
    let fake = FakeApi::start();
    let tok = login(&fake, Duration::from_secs(5), &browser_opener()).unwrap();
    assert_eq!(tok.access_token, "ss_at_c1");
    assert_eq!(tok.token_endpoint, format!("{}/oauth/token", fake.url));
    let auths = fake.authorizations();
    assert_eq!(auths.len(), 1);
    let (_, redirect, scope) = &auths[0];
    assert!(
        redirect.starts_with("http://127.0.0.1:") && redirect.ends_with("/callback"),
        "{redirect}"
    );
    assert_eq!(scope, "starstar:operator");
}

#[test]
fn callback_ignores_stray_requests() {
    let fake = FakeApi::start();
    let opener = |url: &str| {
        let url = url.to_owned();
        std::thread::spawn(move || {
            let u: url::Url = url.parse().unwrap();
            let redirect = u
                .query_pairs()
                .find(|(k, _)| k == "redirect_uri")
                .unwrap()
                .1
                .into_owned();
            let origin = redirect.trim_end_matches("/callback").to_owned();
            assert_eq!(browse(&format!("{origin}/favicon.ico")).0, 404);
            assert_eq!(
                browse(&format!("{origin}/callback?code=x&state=wrong")).0,
                400
            );
            browse(&url);
        });
        Ok(())
    };
    let tok = login(&fake, Duration::from_secs(5), &opener).unwrap();
    assert_eq!(tok.access_token, "ss_at_c1");
}

#[test]
fn denied_sign_in_fails_clearly() {
    let fake = FakeApi::start();
    fake.deny_next();
    let Err(CliError::Auth(m)) = login(&fake, Duration::from_secs(5), &browser_opener()) else {
        panic!("expected an auth error")
    };
    assert!(m.contains("denied"), "{m}");
    assert!(fake.find("POST", "/oauth/token").is_empty());
}

#[test]
fn login_times_out() {
    let fake = FakeApi::start();
    let start = std::time::Instant::now();
    let Err(CliError::Auth(m)) = login(&fake, Duration::from_millis(300), &|_| Ok(())) else {
        panic!("expected an auth error")
    };
    assert!(m.contains("timed out") && m.contains("--no-browser"), "{m}");
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn metadata_on_foreign_origin_is_refused() {
    let fake = FakeApi::start();
    let u = &fake.url;
    fake.set(
        "GET",
        "/.well-known/oauth-authorization-server",
        Canned::json(
            200,
            serde_json::json!({
                "issuer": u,
                "authorization_endpoint": format!("{u}/oauth/authorize"),
                "token_endpoint": "https://evil.example/token",
            }),
        ),
    );
    let opened = std::cell::Cell::new(false);
    let r = login(&fake, Duration::from_secs(1), &|_| {
        opened.set(true);
        Ok(())
    });
    assert!(matches!(r, Err(CliError::Other(_))));
    assert!(!opened.get());
}

#[test]
fn metadata_with_self_consistent_foreign_issuer_is_refused() {
    let fake = FakeApi::start();
    fake.set(
        "GET",
        "/.well-known/oauth-authorization-server",
        Canned::json(
            200,
            serde_json::json!({
                "issuer": "https://evil.example",
                "authorization_endpoint": "https://evil.example/authorize",
                "token_endpoint": "https://evil.example/token",
            }),
        ),
    );
    let opened = std::cell::Cell::new(false);
    let r = login(&fake, Duration::from_secs(1), &|_| {
        opened.set(true);
        Ok(())
    });
    assert!(matches!(r, Err(CliError::Other(_))));
    assert!(!opened.get());
}

// ---- copy-code login, logout, whoami ----

#[test]
fn copy_code_login_end_to_end() {
    let fake = FakeApi::start();
    let api: url::Url = fake.url.parse().unwrap();
    let tok = login_copy_code(
        &build_http().unwrap(),
        &api,
        Role::Operator,
        &mut |url: &str| {
            let u: url::Url = url.parse().unwrap();
            let redirect = u
                .query_pairs()
                .find(|(k, _)| k == "redirect_uri")
                .unwrap()
                .1
                .into_owned();
            assert_eq!(redirect, format!("{}/oauth/code", fake.url));
            let (status, body) = browse(url);
            assert_eq!(status, 200);
            Ok(code_from_page(&body))
        },
    )
    .unwrap();
    assert!(tok.access_token.starts_with("ss_at_c"));
}

#[test]
fn no_browser_login_via_binary() {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::process::Stdio;
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let mut child = cli(&fake, home.path())
        .args(["--json", "login", "--no-browser"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let mut seen = String::new();
    let url = loop {
        let mut line = String::new();
        assert!(
            stderr.read_line(&mut line).unwrap() > 0,
            "no URL on stderr: {seen}"
        );
        seen.push_str(&line);
        if line.contains("/oauth/authorize?") {
            break line.trim().to_owned();
        }
    };
    let (_, body) = browse(&url);
    let code = code_from_page(&body);
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{code}").unwrap();
    drop(stdin);
    let mut rest = String::new();
    stderr.read_to_string(&mut rest).unwrap();
    seen.push_str(&rest);
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    assert!(child.wait().unwrap().success(), "{seen}");
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["username"], "kerry");
    assert!(
        read_credentials(home.path(), &fake)
            .unwrap()
            .refresh_token
            .starts_with("ss_rt_c")
    );
    for text in [&stdout, &seen] {
        assert!(!text.contains(&code), "{text}");
        assert!(
            !text.contains("ss_at_") && !text.contains("ss_rt_"),
            "{text}"
        );
    }
}

#[test]
fn empty_code_is_auth_error() {
    use std::io::Write;
    use std::process::Stdio;
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let mut child = cli(&fake, home.path())
        .args(["login", "--no-browser"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap()).unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(3));
}

#[test]
fn logout_revokes_and_removes() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    fake.add_refresh_token("ss_rt_1");
    write_credentials(
        home.path(),
        &fake,
        stored(&fake, "ss_at_x", "ss_rt_1", future()),
    );
    let out = run(cli(&fake, home.path()).arg("logout"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(fake.revoked(), ["ss_rt_1"]);
    assert!(read_credentials(home.path(), &fake).is_none());
    let again = run(cli(&fake, home.path()).arg("logout"));
    assert_eq!(again.code, 0);
    assert!(again.stdout.contains("Not signed in"), "{}", again.stdout);
}

#[test]
fn logout_offline_still_removes() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let mut t = stored(&fake, "ss_at_x", "ss_rt_1", future());
    t.revocation_endpoint = Some("http://127.0.0.1:1/oauth/revoke".into());
    write_credentials(home.path(), &fake, t);
    let out = run(cli(&fake, home.path()).arg("logout"));
    assert_eq!(out.code, 0);
    assert!(out.stderr.contains("could not revoke"), "{}", out.stderr);
    assert!(read_credentials(home.path(), &fake).is_none());
}

#[test]
fn whoami_json_reports_source() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let out = run(with_token(&fake, home.path(), TOKEN).args(["--json", "whoami"]));
    let v = out.json();
    assert_eq!(
        (v["token_source"].as_str(), v["role"].as_str()),
        (Some("env"), Some("operator"))
    );
    write_credentials(
        home.path(),
        &fake,
        stored(&fake, TOKEN, "ss_rt_1", future()),
    );
    let out = run(cli(&fake, home.path()).args(["--json", "whoami"]));
    assert_eq!(out.json()["token_source"], "credentials");
}

#[test]
fn login_replaces_and_revokes_previous() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let api: url::Url = fake.url.parse().unwrap();
    for _ in 0..2 {
        login_with(
            &api,
            CredentialStore::at(credentials_path(home.path())),
            Role::Operator,
            LoginMethod::Loopback {
                open: &browser_opener(),
                timeout: Duration::from_secs(5),
            },
            &Output { json: true },
        )
        .unwrap();
    }
    assert_eq!(fake.revoked(), ["ss_rt_c1"]);
    assert_eq!(
        read_credentials(home.path(), &fake).unwrap().refresh_token,
        "ss_rt_c2"
    );
}

#[test]
fn failed_me_check_revokes_the_new_grant() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    fake.set(
        "GET",
        "/api/me",
        Canned::json(500, serde_json::json!({"error": "boom"})),
    );
    let api: url::Url = fake.url.parse().unwrap();
    let r = login_with(
        &api,
        CredentialStore::at(credentials_path(home.path())),
        Role::Operator,
        LoginMethod::Loopback {
            open: &browser_opener(),
            timeout: Duration::from_secs(5),
        },
        &Output { json: true },
    );
    assert!(r.is_err());
    assert_eq!(fake.revoked(), ["ss_rt_c1"]);
    assert!(read_credentials(home.path(), &fake).is_none());
}

#[test]
fn setup_creates_first_admin() {
    use std::io::Write;
    use std::process::Stdio;
    let fake = FakeApi::start();
    fake.set(
        "POST",
        "/api/setup",
        Canned::json(
            201,
            serde_json::json!({"username": "boss", "role": "admin", "csrf_token": "csrf-secret"}),
        ),
    );
    let home = tempfile::tempdir().unwrap();
    let mut child = cli(&fake, home.path())
        .args([
            "--json",
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
    write!(child.stdin.take().unwrap(), "code-123\nlong-password-1\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("csrf-secret") && !stdout.contains("long-password-1"));
    let body = fake.find("POST", "/api/setup")[0].json();
    assert_eq!(
        body,
        serde_json::json!({"setup_code": "code-123", "username": "boss", "password": "long-password-1"})
    );
}
