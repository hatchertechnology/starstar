mod support;

use serde_json::json;
use support::*;

fn tok(fake: &FakeApi, home: &std::path::Path) -> std::process::Command {
    let mut c = cli(fake, home);
    c.env("STARSTAR_TOKEN", TOKEN);
    c
}

#[test]
fn check_json() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["--json", "update", "check", "mk4"]));
    let v = out.json();
    assert_eq!(
        (v["available"].as_bool(), v["new_version"].as_str()),
        (Some(true), Some("0.8.1"))
    );
}

#[test]
fn check_up_to_date_human() {
    let fake = FakeApi::start();
    fake.set(
        "GET",
        "/api/printers/mk4/update/prusalink",
        Canned::json(200, json!({"env": "prusalink", "available": false})),
    );
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["update", "check", "mk4"]));
    assert!(out.stdout.contains("up to date"), "{}", out.stdout);
}

#[test]
fn apply_without_yes_refuses() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["update", "apply", "mk4"]));
    assert_eq!(out.code, 5);
    assert!(fake.requests().is_empty());
}

#[test]
fn apply_with_yes() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["--json", "update", "apply", "mk4", "--yes"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.json()["action"], "apply");
    assert_eq!(
        fake.find("POST", "/api/printers/mk4/update/prusalink")
            .len(),
        1
    );
}

#[test]
fn unknown_env_is_usage_error() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["update", "check", "mk4", "--env", "system"]));
    assert_eq!(out.code, 2);
}

#[test]
fn apply_needs_admin_message() {
    let fake = FakeApi::start();
    fake.set(
        "POST",
        "/api/printers/mk4/update/prusalink",
        Canned::json(403, json!({"error": "requires the admin role"})),
    );
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["update", "apply", "mk4", "--yes"]));
    assert_eq!(out.code, 4);
}
