mod support;
use serde_json::json;
use support::*;

fn summary(id: i64, name: &str) -> serde_json::Value {
    json!({"id":id,"name":name,"latest_model_revision":1,"setups":1,"slices":1,"preview_file_id":null,"updated_at":1})
}
fn tree() -> serde_json::Value {
    json!({"id":7,"name":"cube","notes":null,"default_setup_id":null,"created_by":"test","created_at":1,"preview_file_id":null,"model_revisions":[{"id":11,"number":1,"note":null,"created_by":"test","created_at":1,"files":[]}],"setups":[{"id":12,"name":"standard","default_profile":null,"behind_model":null,"revisions":[{"id":13,"number":1,"model_revision_id":11,"model_revision_number":1,"note":null,"created_by":"test","created_at":1,"project":null,"slices":[]}]}],"other_files":[]})
}
fn setup() -> (FakeApi, tempfile::TempDir) {
    let f = FakeApi::start();
    f.set(
        "GET",
        "/api/parts",
        Canned::json(200, vec![summary(7, "cube")]),
    );
    f.set("GET", "/api/parts/7", Canned::json(200, tree()));
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
    let o = run(tok(&f, h.path()).args(["part", "ls"]));
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(o.stdout.contains("cube") && o.stdout.contains("ID"));
}
#[test]
fn upload_slice_resolves_setup_name() {
    let (f, h) = setup();
    let p = h.path().join("cube.gcode");
    std::fs::write(&p, b"abc").unwrap();
    f.set(
        "POST",
        "/api/parts/7/files?name=cube.gcode&role=slice&setup_revision=13",
        Canned::json(201, json!({})),
    );
    let o = run(tok(&f, h.path())
        .args(["part", "upload", "7"])
        .arg(&p)
        .args(["--setup", "standard"]));
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(
        f.find(
            "POST",
            "/api/parts/7/files?name=cube.gcode&role=slice&setup_revision=13"
        )
        .len(),
        1
    );
}
#[test]
fn upload_infers_role() {
    let (f, h) = setup();
    let p = h.path().join("cube.stl");
    std::fs::write(&p, b"abc").unwrap();
    f.set(
        "POST",
        "/api/parts/7/files?name=cube.stl&role=source",
        Canned::json(201, json!({})),
    );
    let o = run(tok(&f, h.path()).args(["part", "upload", "7"]).arg(&p));
    assert_eq!(o.code, 0, "{}", o.stderr);
}
#[test]
fn upload_3mf_without_setup_errors() {
    let (f, h) = setup();
    let p = h.path().join("cube.3mf");
    std::fs::write(&p, b"abc").unwrap();
    let o = run(tok(&f, h.path()).args(["part", "upload", "7"]).arg(&p));
    assert_ne!(o.code, 0);
    assert!(o.stderr.contains("--setup"), "{}", o.stderr);
}
#[test]
fn rm_needs_yes_without_terminal() {
    let (f, h) = setup();
    let o = run(tok(&f, h.path()).args(["part", "rm", "7"]));
    assert_eq!(o.code, 5);
    assert!(f.requests().is_empty());
}
#[test]
fn part_by_name_ambiguous_errors() {
    let (f, h) = setup();
    f.set(
        "GET",
        "/api/parts?q=cube",
        Canned::json(200, vec![summary(7, "cube"), summary(8, "Cube")]),
    );
    let o = run(tok(&f, h.path()).args(["part", "info", "cube"]));
    assert_ne!(o.code, 0);
    assert!(o.stderr.contains("ambiguous"), "{}", o.stderr);
}

#[test]
fn new_and_revision_send_expected_json() {
    let (f, h) = setup();
    f.set("POST", "/api/parts", Canned::json(201, tree()));
    let o = run(tok(&f, h.path()).args(["part", "new", "cube", "--notes", "note", "--json"]));
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(o.json()["id"], 7);
    let requests = f.find("POST", "/api/parts");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&requests[0].body).unwrap(),
        json!({"name":"cube","notes":"note"})
    );
    f.set(
        "POST",
        "/api/parts/7/revisions",
        Canned::json(201, tree()["model_revisions"][0].clone()),
    );
    let o = run(tok(&f, h.path()).args([
        "part", "rev", "7", "--note", "change", "--carry", "42", "--carry", "43",
    ]));
    assert_eq!(o.code, 0, "{}", o.stderr);
    let requests = f.find("POST", "/api/parts/7/revisions");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&requests[0].body).unwrap(),
        json!({"note":"change","carry":[42,43]})
    );
}

#[test]
fn download_preserves_existing_file_and_checks_size() {
    let (f, h) = setup();
    let file = json!({"id":9,"part_id":7,"model_revision_id":11,"setup_revision_id":null,"role":"source","name":"cube.stl","sha256":"abc","size":3,"meta":null,"parsed":false,"has_preview":false,"target":null,"superseded_by":null,"note":null,"uploaded_by":"test","uploaded_at":1});
    f.set("GET", "/api/files/9", Canned::json(200, file));
    f.set(
        "GET",
        "/api/files/9/download",
        Canned::bytes(200, "application/octet-stream", b"ab".to_vec()),
    );
    let path = h.path().join("existing");
    std::fs::write(&path, b"old").unwrap();
    let o = run(tok(&f, h.path())
        .args(["part", "download", "9", "--force", "-o"])
        .arg(&path));
    assert_ne!(o.code, 0);
    assert!(o.stderr.contains("size mismatch"));
    assert_eq!(std::fs::read(&path).unwrap(), b"old");
    f.set(
        "GET",
        "/api/files/9/download",
        Canned::bytes(200, "application/octet-stream", b"abc".to_vec()),
    );
    let o = run(tok(&f, h.path())
        .args(["part", "download", "9", "-o"])
        .arg(&path));
    assert_ne!(o.code, 0);
    assert_eq!(std::fs::read(&path).unwrap(), b"old");
    let o = run(tok(&f, h.path())
        .args(["part", "download", "9", "--force", "-o"])
        .arg(&path));
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(std::fs::read(path).unwrap(), b"abc");
}
