mod support;

use serde_json::json;
use support::*;

fn tok(fake: &FakeApi, home: &std::path::Path) -> std::process::Command {
    let mut c = cli(fake, home);
    c.env("STARSTAR_TOKEN", TOKEN);
    c
}

#[test]
fn ls_root_json() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["files", "ls", "mk4", "usb", "--json"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    let v = out.json();
    assert_eq!(v["kind"], "folder");
    assert_eq!(v["children"][0]["display_name"], "cube-12mm.bgcode");
    assert_eq!(fake.find("GET", "/api/printers/mk4/files/usb").len(), 1);
}

#[test]
fn ls_human_lists_children() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["files", "ls", "mk4", "usb"]));
    assert!(
        out.stdout.contains("cube-12mm.bgcode") && out.stdout.contains("parts"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("folder"));
}

#[test]
fn info_merges_head_state() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out =
        run(tok(&fake, h.path()).args(["--json", "files", "info", "mk4", "usb/cube-12mm.bgcode"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    let v = out.json();
    assert_eq!(v["meta"]["filament_type"], "PLA");
    assert_eq!(v["state"]["currently_printed"], true);
    assert_eq!(v["state"]["read_only"], false);
    assert_eq!(
        fake.find("HEAD", "/api/printers/mk4/files/usb/cube-12mm.bgcode")
            .len(),
        1
    );
    let human = run(tok(&fake, h.path()).args(["files", "info", "mk4", "usb/cube-12mm.bgcode"]));
    assert!(
        human.stdout.contains("printing now: yes"),
        "{}",
        human.stdout
    );
}

#[test]
fn info_on_folder_skips_head() {
    let fake = FakeApi::start();
    fake.set(
        "GET",
        "/api/printers/mk4/files/usb/parts",
        Canned::json(200, entry("parts", starstar_shared::FileKind::Folder, None)),
    );
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["--json", "files", "info", "mk4", "usb/parts"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.json().get("state").is_none());
    assert!(fake.requests().iter().all(|r| r.method != "HEAD"));
}

#[test]
fn info_needs_a_file_path() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["files", "info", "mk4", "usb"]));
    assert_eq!(out.code, 2);
}

#[test]
fn print_posts_print_suffix() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out =
        run(tok(&fake, h.path()).args(["--json", "files", "print", "mk4", "usb/cube-12mm.bgcode"]));
    assert_eq!(out.json()["action"], "print");
    assert_eq!(
        fake.find("POST", "/api/printers/mk4/files/usb/cube-12mm.bgcode/print")
            .len(),
        1
    );
}

#[test]
fn rm_without_yes_sends_nothing() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["files", "rm", "mk4", "usb/cube-12mm.bgcode"]));
    assert_eq!(out.code, 5);
    assert!(fake.requests().is_empty());
}

#[test]
fn rm_force_with_yes() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out =
        run(tok(&fake, h.path()).args(["files", "rm", "mk4", "usb/parts", "--force", "--yes"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        fake.find("DELETE", "/api/printers/mk4/files/usb/parts?force=true")
            .len(),
        1
    );
    let out = run(tok(&fake, h.path()).args(["files", "rm", "mk4", "usb/parts", "--yes"]));
    assert_eq!(out.code, 0);
    assert_eq!(
        fake.find("DELETE", "/api/printers/mk4/files/usb/parts")
            .len(),
        1
    );
}

#[test]
fn paths_with_reserved_characters_round_trip() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out =
        run(tok(&fake, h.path()).args(["files", "print", "mk4", "usb/My Parts/café #1?%.bgcode"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        fake.find(
            "POST",
            "/api/printers/mk4/files/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode/print"
        )
        .len(),
        1
    );
}

#[test]
fn unsafe_paths_are_refused_locally() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    for bad in ["usb/../x", "usb//x", "usb/a\\b"] {
        let out = run(tok(&fake, h.path()).args(["files", "info", "mk4", bad]));
        assert_eq!(out.code, 2, "{bad}: {}", out.stderr);
    }
    assert!(fake.api_requests().is_empty());
}

#[test]
fn missing_file_is_404_message() {
    let fake = FakeApi::start();
    fake.set(
        "GET",
        "/api/printers/mk4/files/usb/nope.bgcode",
        Canned::json(404, json!({"error": "not found on the printer"})),
    );
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["files", "info", "mk4", "usb/nope.bgcode"]));
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("not found on the printer"));
}
