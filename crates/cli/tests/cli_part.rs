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
    f.set(
        "GET",
        "/api/parts?q=7",
        Canned::json(200, Vec::<serde_json::Value>::new()),
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
        "/api/parts/7/files?name=cube.gcode&role=slice&setup=12",
        Canned::json(201, json!({})),
    );
    let o = run(tok(&f, h.path())
        .args(["part", "upload", "7"])
        .arg(&p)
        .args(["--setup", "standard"]));
    assert_eq!(o.code, 0, "{}", o.stderr);
    let requests = f.find(
        "POST",
        "/api/parts/7/files?name=cube.gcode&role=slice&setup=12",
    );
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].body, b"abc");
    assert_eq!(requests[0].header("content-length"), Some("3"));
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

#[test]
fn numeric_part_names_require_explicit_disambiguation_before_delete() {
    let (f, h) = setup();
    f.set(
        "GET",
        "/api/parts?q=42",
        Canned::json(200, vec![summary(7, "42")]),
    );
    let o = run(tok(&f, h.path()).args(["part", "rm", "42", "--yes"]));
    assert_ne!(o.code, 0);
    assert!(
        o.stderr.contains("ambiguous part 42: use id:42 or name:42"),
        "{}",
        o.stderr
    );
    assert!(f.find("DELETE", "/api/parts/42").is_empty());
    let o = run(tok(&f, h.path()).args(["part", "info", "name:42"]));
    assert_eq!(o.code, 0, "{}", o.stderr);
    let o = run(tok(&f, h.path()).args(["part", "info", "id:7"]));
    assert_eq!(o.code, 0, "{}", o.stderr);
    f.set(
        "GET",
        "/api/parts?q=7",
        Canned::json(200, vec![summary(7, "7")]),
    );
    let o = run(tok(&f, h.path()).args(["part", "info", "7"]));
    assert_eq!(o.code, 0, "{}", o.stderr);
}

#[test]
fn info_marks_old_slices_and_filters_replaced_unless_all() {
    let (f, h) = setup();
    let mut p = tree();
    let slice = |id, superseded| json!({"id":id,"part_id":7,"model_revision_id":null,"setup_revision_id":13,"role":"slice","name":"cube.gcode","sha256":"abc","size":3,"meta":null,"parsed":false,"has_preview":false,"target":null,"superseded_by":superseded,"note":null,"uploaded_by":"test","uploaded_at":1});
    p["setups"][0]["revisions"][0]["slices"] =
        json!([slice(101, None::<i64>), slice(102, Some(103))]);
    let mut newest = p["setups"][0]["revisions"][0].clone();
    newest["id"] = json!(14);
    newest["number"] = json!(2);
    newest["slices"] = json!([slice(103, None::<i64>)]);
    p["setups"][0]["revisions"]
        .as_array_mut()
        .unwrap()
        .push(newest);
    f.set("GET", "/api/parts/7", Canned::json(200, p));
    let o = run(tok(&f, h.path()).args(["part", "info", "id:7"]));
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(
        o.stdout
            .lines()
            .any(|s| s.contains("101 ") && s.contains("from r1")),
        "{}",
        o.stdout
    );
    assert!(!o.stdout.contains("102 "));
    assert!(o.stdout.contains("103 "));
    let o = run(tok(&f, h.path()).args(["part", "info", "id:7", "--all"]));
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(
        o.stdout
            .lines()
            .any(|s| s.contains("102 ") && s.contains("replaced") && s.contains("from r1")),
        "{}",
        o.stdout
    );
    let o = run(tok(&f, h.path()).args(["part", "info", "id:7", "--json"]));
    assert_eq!(
        o.json()["setups"][0]["revisions"][0]["slices"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn explicit_setup_revision_is_resolved() {
    let (f, h) = setup();
    let p = h.path().join("cube.gcode");
    std::fs::write(&p, b"abc").unwrap();
    let path = "/api/parts/7/files?name=cube.gcode&role=slice&setup_revision=13";
    f.set("POST", path, Canned::json(201, json!({})));
    let o = run(tok(&f, h.path())
        .args(["part", "upload", "id:7"])
        .arg(&p)
        .args(["--setup", "standard", "--setup-rev", "1"]));
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(f.find("POST", path).len(), 1);
}
