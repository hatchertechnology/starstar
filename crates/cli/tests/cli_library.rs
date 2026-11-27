mod support;
use serde_json::json;
use support::*;

fn file() -> serde_json::Value {
    json!({"id":7,"name":"cube.bgcode","version":1,"sha256":"abc","size":3,"kind":"bgcode","meta":{"printer_model":"MK4","filament_type":"PLA","estimated_print_time":60},"parsed":true,"has_preview":false,"uploaded_by":"test","uploaded_at":1})
}
fn setup() -> (FakeApi, tempfile::TempDir) {
    let f = FakeApi::start();
    f.set("GET", "/api/library", Canned::json(200, vec![file()]));
    f.set("GET", "/api/library/7", Canned::json(200, file()));
    f.set(
        "GET",
        "/api/library/7/download",
        Canned::bytes(200, "application/octet-stream", b"abc".to_vec()),
    );
    f.set(
        "POST",
        "/api/library?name=cube.bgcode",
        Canned::json(201, file()),
    );
    (f, tempfile::tempdir().unwrap())
}
fn tok(f: &FakeApi, h: &std::path::Path) -> std::process::Command {
    let mut c = cli(f, h);
    c.env("STARSTAR_TOKEN", TOKEN);
    c
}
#[test]
fn ls_prints_table() {
    let (f, h) = setup();
    let o = run(tok(&f, h.path()).args(["library", "ls"]));
    assert_eq!(o.code, 0, "{}", o.stderr);
    for s in [
        "ID",
        "NAME",
        "VERSION",
        "SLICED FOR",
        "FILAMENT",
        "TIME",
        "cube.bgcode",
        "MK4",
        "PLA",
    ] {
        assert!(o.stdout.contains(s), "{}", o.stdout);
    }
    assert!(
        o.stdout
            .lines()
            .any(|line| line.starts_with("7 ") && line.contains("cube.bgcode"))
    );
}
#[test]
fn upload_streams_with_length() {
    let (f, h) = setup();
    let p = h.path().join("cube.bgcode");
    std::fs::write(&p, b"abc").unwrap();
    let o = run(tok(&f, h.path()).arg("library").arg("upload").arg(&p));
    assert_eq!(o.code, 0, "{}", o.stderr);
    let r = f.find("POST", "/api/library?name=cube.bgcode");
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].body, b"abc");
    assert_eq!(r[0].header("content-length"), Some("3"));
}
#[test]
fn download_refuses_overwrite_without_force() {
    let (f, h) = setup();
    let p = h.path().join("existing");
    std::fs::write(&p, b"old").unwrap();
    let o = run(tok(&f, h.path())
        .args(["library", "download", "7", "-o"])
        .arg(&p));
    assert_ne!(o.code, 0);
    assert_eq!(std::fs::read(&p).unwrap(), b"old");
    let o = run(tok(&f, h.path())
        .args(["library", "download", "7", "--force", "-o"])
        .arg(&p));
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(std::fs::read(p).unwrap(), b"abc");
}
#[test]
fn rm_needs_yes_without_tty() {
    let (f, h) = setup();
    let o = run(tok(&f, h.path()).args(["library", "rm", "7"]));
    assert_eq!(o.code, 5);
    assert!(f.requests().is_empty());
}
#[test]
fn json_flag_prints_json() {
    let (f, h) = setup();
    let o = run(tok(&f, h.path()).args(["library", "ls", "--json"]));
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(o.json()[0]["id"], 7);
}

#[test]
fn failed_download_with_force_preserves_destination() {
    let (f, h) = setup();
    f.set(
        "GET",
        "/api/library/7/download",
        Canned::json(500, json!({"error": "failed"})),
    );
    let p = h.path().join("existing");
    std::fs::write(&p, b"original").unwrap();
    let o = run(tok(&f, h.path())
        .args(["library", "download", "7", "--force", "-o"])
        .arg(&p));
    assert_ne!(o.code, 0);
    assert_eq!(std::fs::read(p).unwrap(), b"original");
}

#[test]
fn mismatched_download_with_force_preserves_destination() {
    for body in [b"ab".as_slice(), b"abcd".as_slice()] {
        let (f, h) = setup();
        f.set(
            "GET",
            "/api/library/7/download",
            Canned::bytes(200, "application/octet-stream", body.to_vec()),
        );
        let p = h.path().join("existing");
        std::fs::write(&p, b"original").unwrap();
        let o = run(tok(&f, h.path())
            .args(["library", "download", "7", "--force", "-o"])
            .arg(&p));
        assert_ne!(o.code, 0);
        assert!(o.stderr.contains("download size mismatch"), "{}", o.stderr);
        assert_eq!(std::fs::read(&p).unwrap(), b"original");
    }
}
