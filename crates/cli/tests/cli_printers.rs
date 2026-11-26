mod support;

use serde_json::json;
use support::*;

fn with_token(fake: &FakeApi, home: &std::path::Path) -> std::process::Command {
    let mut c = cli(fake, home);
    c.env("STARSTAR_TOKEN", TOKEN);
    c
}

#[test]
fn lists_printers_json() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let out = run(with_token(&fake, home.path()).args(["--json", "printer", "list"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.json()[0]["name"], "mk4");
    let reqs = fake.find("GET", "/api/printers");
    assert_eq!(reqs[0].header("authorization"), Some("Bearer ss_at_test"));
}

#[test]
fn lists_printers_human() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let out = run(with_token(&fake, home.path()).args(["printer", "list"]));
    assert!(
        out.stdout.contains("mk4") && out.stdout.contains("10.0.0.5"),
        "{}",
        out.stdout
    );
}

#[test]
fn show_info_status_storage_json() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let j = |args: &[&str]| {
        let out = run(with_token(&fake, home.path()).arg("--json").args(args));
        assert_eq!(out.code, 0, "{}", out.stderr);
        out.json()
    };
    assert_eq!(j(&["printer", "show", "mk4"])["kind"], "mk4");
    assert!(j(&["printer", "info", "mk4"])["firmware"].is_string());
    assert_eq!(j(&["printer", "status", "mk4"])["state"], "PRINTING");
    assert_eq!(j(&["printer", "storage", "mk4"])[0]["path"], "/usb");
}

#[test]
fn invalid_printer_name_is_usage_error() {
    let fake = FakeApi::start();
    let home = tempfile::tempdir().unwrap();
    let out = run(with_token(&fake, home.path()).args(["printer", "show", "Bad_Name"]));
    assert_eq!(out.code, 2);
    assert!(fake.api_requests().is_empty());
}

#[test]
fn forbidden_exits_4() {
    let fake = FakeApi::start();
    fake.set(
        "GET",
        "/api/printers",
        Canned::json(403, json!({"error": "requires the admin role"})),
    );
    let home = tempfile::tempdir().unwrap();
    let out = run(with_token(&fake, home.path()).args(["printer", "list"]));
    assert_eq!(out.code, 4);
    assert!(out.stderr.contains("admin role"), "{}", out.stderr);
}
