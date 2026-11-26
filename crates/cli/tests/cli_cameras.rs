mod support;

use serde_json::json;
use support::*;

fn tok(fake: &FakeApi, home: &std::path::Path) -> std::process::Command {
    let mut c = cli(fake, home);
    c.env("STARSTAR_TOKEN", TOKEN);
    c
}

#[test]
fn ls_and_show_json() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let ls = run(tok(&fake, h.path()).args(["--json", "camera", "ls", "mk4"]));
    assert_eq!(ls.json()[0]["id"], "cam1");
    let show = run(tok(&fake, h.path()).args(["--json", "camera", "show", "mk4", "cam1"]));
    assert_eq!(show.json()["trigger_scheme"], "thirty_sec");
    let human = run(tok(&fake, h.path()).args(["camera", "ls", "mk4"]));
    assert!(human.stdout.contains("cam1") && human.stdout.contains("connected=true"));
}

#[test]
fn default_snapshot_to_file() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let file = h.path().join("s.png");
    let out = run(tok(&fake, h.path())
        .args(["camera", "snap", "mk4", "--json", "-o"])
        .arg(&file));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(std::fs::read(&file).unwrap(), PNG);
    assert!(out.json()["camera"].is_null());
    assert_eq!(fake.find("GET", "/api/printers/mk4/cameras/snap").len(), 1);
}

#[test]
fn take_snapshot_posts() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let file = h.path().join("s.png");
    let out = run(tok(&fake, h.path())
        .args(["camera", "snap", "mk4", "cam1", "--take", "-o"])
        .arg(&file));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        fake.find("POST", "/api/printers/mk4/cameras/cam1/snap")
            .len(),
        1
    );
}

#[test]
fn snapshot_to_stdout() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let o = tok(&fake, h.path())
        .args(["camera", "snap", "mk4", "cam1", "-o", "-"])
        .output()
        .unwrap();
    assert!(o.status.success());
    assert_eq!(o.stdout, PNG);
    let with_json = run(tok(&fake, h.path()).args(["--json", "camera", "snap", "mk4", "-o", "-"]));
    assert_eq!(with_json.code, 2);
}

#[test]
fn no_snapshot_yet_exits_1() {
    let fake = FakeApi::start();
    fake.set("GET", "/api/printers/mk4/cameras/snap", Canned::empty(204));
    let h = tempfile::tempdir().unwrap();
    let file = h.path().join("s.png");
    let out = run(tok(&fake, h.path())
        .args(["camera", "snap", "mk4", "-o"])
        .arg(&file));
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("no snapshot yet"), "{}", out.stderr);
    assert!(!file.exists());
}

#[test]
fn take_requires_id() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["camera", "snap", "mk4", "--take", "-o", "x"]));
    assert_eq!(out.code, 2);
}

#[test]
fn config_sends_settings_json() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args([
        "camera",
        "config",
        "mk4",
        "cam1",
        "--trigger",
        "each_layer",
        "--resolution",
        "640x480",
    ]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    let reqs = fake.find("PATCH", "/api/printers/mk4/cameras/cam1/config");
    assert_eq!(
        reqs[0].json(),
        json!({"trigger_scheme": "each_layer", "resolution": {"width": 640, "height": 480}})
    );
    let out = run(tok(&fake, h.path()).args([
        "camera",
        "config",
        "mk4",
        "cam1",
        "--rotation",
        "-90",
        "--send-to-connect",
        "false",
    ]));
    assert_eq!(out.code, 0, "{}", out.stderr);
}

#[test]
fn config_without_settings_is_usage_error() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["camera", "config", "mk4", "cam1"]));
    assert_eq!(out.code, 2);
    assert!(fake.requests().is_empty());
}

#[test]
fn order_sends_array() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["--json", "camera", "order", "mk4", "cam2", "cam1"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        fake.find("PUT", "/api/printers/mk4/cameras/order")[0].json(),
        json!(["cam2", "cam1"])
    );
    assert_eq!(out.json()["order"], json!(["cam2", "cam1"]));
}

#[test]
fn camera_writes_forward() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let table = [
        ("register", "POST", "/api/printers/mk4/cameras/cam1"),
        ("remove", "DELETE", "/api/printers/mk4/cameras/cam1"),
        ("reset", "DELETE", "/api/printers/mk4/cameras/cam1/config"),
        (
            "connect",
            "POST",
            "/api/printers/mk4/cameras/cam1/connection",
        ),
        (
            "disconnect",
            "DELETE",
            "/api/printers/mk4/cameras/cam1/connection",
        ),
    ];
    for (verb, method, uri) in table {
        let out = run(tok(&fake, h.path()).args(["--json", "camera", verb, "mk4", "cam1"]));
        assert_eq!(out.code, 0, "{verb}: {}", out.stderr);
        assert_eq!(out.json()["action"], verb);
        assert_eq!(fake.find(method, uri).len(), 1, "{verb}");
    }
}

#[test]
fn reserved_camera_ids_refused() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["camera", "show", "mk4", "snap"]));
    assert_eq!(out.code, 2);
    assert!(fake.requests().is_empty());
}
