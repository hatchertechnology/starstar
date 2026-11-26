mod support;

use serde_json::json;
use support::*;

#[test]
fn status_reports_health() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let out = run(cli(&fake, home.path()).arg("status"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.contains("starstar API"), "{}", out.stdout);
    assert!(out.stdout.contains("is ok"), "{}", out.stdout);
}

#[test]
fn status_json() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let out = run(cli(&fake, home.path()).args(["--json", "status"]));
    assert_eq!(out.json()["status"], "ok");
}

#[test]
fn unreachable_api_exits_1() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let out = run(cli(&fake, home.path())
        .env("STARSTAR_API_URL", "http://127.0.0.1:1")
        .arg("status"));
    assert_eq!(out.code, 1);
    assert!(
        out.stderr.contains("could not reach the starstar API"),
        "{}",
        out.stderr
    );
}

#[test]
fn api_error_message_is_shown() {
    let fake = FakeApi::start();
    fake.set(
        "GET",
        "/api/health",
        Canned::json(503, json!({"error": "db down"})),
    );
    let home = tempfile::tempdir().unwrap();
    let out = run(cli(&fake, home.path()).arg("status"));
    assert_eq!(out.code, 1);
    assert_eq!(out.stderr, "error: db down\n");
}

#[test]
fn json_errors_are_json_on_stderr() {
    let fake = FakeApi::start();
    fake.set(
        "GET",
        "/api/health",
        Canned::json(503, json!({"error": "db down"})),
    );
    let home = tempfile::tempdir().unwrap();
    let out = run(cli(&fake, home.path()).args(["--json", "status"]));
    assert_eq!(out.code, 1);
    assert!(out.stdout.is_empty());
    let v: serde_json::Value = serde_json::from_str(&out.stderr).unwrap();
    assert_eq!(v["error"], "db down");
    assert_eq!(v["http_status"], 503);
}

#[test]
fn bad_api_url_is_usage_error() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let out = run(cli(&fake, home.path()).args(["--api-url", "ftp://x", "status"]));
    assert_eq!(out.code, 2);
}

#[test]
fn usage_errors_are_json_with_json_flag() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let out = run(cli(&fake, home.path()).args(["--json", "nonsense"]));
    assert_eq!(out.code, 2);
    let v: serde_json::Value = serde_json::from_str(&out.stderr).unwrap();
    assert!(v["error"].as_str().unwrap().contains("nonsense"));
    assert!(v["http_status"].is_null());
}

/// A proxy address nothing listens on.
fn dead_proxy() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}", l.local_addr().unwrap())
}

/// The CLI ignores proxy variables, so a dead proxy and no `NO_PROXY` must
/// not break a request (the sign-in, refresh and API traffic all share one client).
fn with_dead_proxy(cmd: &mut std::process::Command) -> &mut std::process::Command {
    let proxy = dead_proxy();
    for var in ["HTTP_PROXY", "ALL_PROXY", "http_proxy", "all_proxy"] {
        cmd.env(var, &proxy);
    }
    cmd.env_remove("NO_PROXY").env_remove("no_proxy")
}

#[test]
fn dead_proxy_does_not_break_the_health_check() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let out = run(with_dead_proxy(&mut cli(&fake, home.path())).arg("status"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.contains("is ok"), "{}", out.stdout);
}

#[test]
fn dead_proxy_does_not_break_a_token_refresh() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    fake.add_refresh_token("ss_rt_1");
    write_credentials(
        home.path(),
        &fake,
        stored(&fake, "ss_at_stale", "ss_rt_1", future()),
    );
    let out = run(with_dead_proxy(&mut cli(&fake, home.path())).args(["printer", "list"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(fake.refresh_count(), 1);
}
