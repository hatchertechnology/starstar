mod support;

use serde_json::json;
use support::*;

fn tok(fake: &FakeApi, home: &std::path::Path) -> std::process::Command {
    let mut c = cli(fake, home);
    c.env("STARSTAR_TOKEN", TOKEN);
    c
}

#[test]
fn job_show_json() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["--json", "job", "show", "mk4"]));
    let v = out.json();
    assert_eq!(v["id"], 42);
    assert_eq!(v["file"]["display_name"], "cube-12mm.bgcode");
}

#[test]
fn job_show_human() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["job", "show", "mk4"]));
    for want in ["job 42", "37.5%", "30m left"] {
        assert!(out.stdout.contains(want), "{}", out.stdout);
    }
}

#[test]
fn idle_printer_job_show_is_null() {
    let fake = FakeApi::start();
    fake.set("GET", "/api/printers/mk4/job", Canned::empty(204));
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["--json", "job", "show", "mk4"]));
    assert_eq!(out.code, 0);
    assert!(out.json().is_null());
    let human = run(tok(&fake, h.path()).args(["job", "show", "mk4"]));
    assert!(human.stdout.contains("no job"));
}

#[test]
fn pause_looks_up_current_job() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["--json", "job", "pause", "mk4"]));
    assert_eq!(
        out.json(),
        json!({"printer": "mk4", "job_id": 42, "action": "pause"})
    );
    let seen: Vec<_> = fake
        .api_requests()
        .iter()
        .map(|r| format!("{} {}", r.method, r.uri))
        .collect();
    assert_eq!(
        seen,
        [
            "GET /api/printers/mk4/job",
            "PUT /api/printers/mk4/job/42/pause"
        ]
    );
}

#[test]
fn resume_and_continue_with_id_skip_lookup() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    for (verb, id) in [("resume", "9"), ("continue", "9")] {
        let out = run(tok(&fake, h.path()).args(["job", verb, "mk4", "--id", id]));
        assert_eq!(out.code, 0, "{}", out.stderr);
    }
    assert_eq!(fake.find("PUT", "/api/printers/mk4/job/9/resume").len(), 1);
    assert_eq!(
        fake.find("PUT", "/api/printers/mk4/job/9/continue").len(),
        1
    );
    assert!(fake.find("GET", "/api/printers/mk4/job").is_empty());
}

#[test]
fn pause_with_no_job_exits_1() {
    let fake = FakeApi::start();
    fake.set("GET", "/api/printers/mk4/job", Canned::empty(204));
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["job", "pause", "mk4"]));
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("no job is running"), "{}", out.stderr);
    assert!(fake.api_requests().iter().all(|r| r.method != "PUT"));
}

#[test]
fn busy_printer_is_reported() {
    let fake = FakeApi::start();
    fake.set(
        "PUT",
        "/api/printers/mk4/job/42/pause",
        Canned::json(
            409,
            json!({"error": "printer is busy or the target is in use"}),
        ),
    );
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["job", "pause", "mk4"]));
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("busy"));
}

#[test]
fn stop_without_yes_sends_nothing() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["job", "stop", "mk4"]));
    assert_eq!(out.code, 5);
    assert!(fake.requests().is_empty());
}

#[test]
fn stop_with_yes() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["--json", "job", "stop", "mk4", "--yes"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.json()["action"], "stop");
    assert_eq!(fake.find("DELETE", "/api/printers/mk4/job/42").len(), 1);
}

#[test]
fn transfer_show_and_cancel() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let show = run(tok(&fake, h.path()).args(["--json", "transfer", "show", "mk4"]));
    assert_eq!(show.json()["id"], 7);
    let cancel = run(tok(&fake, h.path()).args(["--json", "transfer", "cancel", "mk4"]));
    assert_eq!(cancel.json()["transfer_id"], 7);
    assert_eq!(fake.find("DELETE", "/api/printers/mk4/transfer/7").len(), 1);
}

#[test]
fn transfer_without_id_needs_flag() {
    let fake = FakeApi::start();
    fake.set(
        "GET",
        "/api/printers/mk4/transfer",
        Canned::json(200, transfer(None)),
    );
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["transfer", "cancel", "mk4"]));
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("pass --id"), "{}", out.stderr);
    let ok = run(tok(&fake, h.path()).args(["transfer", "cancel", "mk4", "--id", "8"]));
    assert_eq!(ok.code, 0, "{}", ok.stderr);
    assert_eq!(fake.find("DELETE", "/api/printers/mk4/transfer/8").len(), 1);
}
