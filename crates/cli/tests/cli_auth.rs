mod support;

use std::time::Duration;

use starstar_cli::client::ApiClient;
use starstar_cli::endpoints;
use starstar_cli::session::{PlainHttpWarner, TokenSource};
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
