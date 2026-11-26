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

// ---- upload ----

fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 256) as u8).collect()
}

#[test]
fn upload_streams_whole_file_with_length() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let data = pattern(3 * 1024 * 1024);
    let file = h.path().join("big.bgcode");
    std::fs::write(&file, &data).unwrap();
    let out = run(tok(&fake, h.path())
        .args(["files", "upload", "mk4", "usb/"])
        .arg(&file)
        .args(["--print", "--json"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    let puts = fake.find("PUT", "/api/printers/mk4/files/usb/big.bgcode?print=true");
    assert_eq!(puts.len(), 1);
    assert_eq!(puts[0].body, data);
    assert_eq!(puts[0].header("content-length"), Some("3145728"));
    assert!(puts[0].header("transfer-encoding").is_none());
    assert_eq!(
        puts[0].header("content-type"),
        Some("application/octet-stream")
    );
    let v = out.json();
    assert_eq!(v["bytes"], 3145728);
    assert_eq!(v["print"], true);
}

#[test]
fn upload_to_explicit_path_with_overwrite() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let file = h.path().join("local.bgcode");
    std::fs::write(&file, b"x").unwrap();
    let out = run(tok(&fake, h.path())
        .args(["files", "upload", "mk4", "usb/parts/cube.bgcode"])
        .arg(&file)
        .arg("--overwrite"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        fake.find(
            "PUT",
            "/api/printers/mk4/files/usb/parts/cube.bgcode?overwrite=true"
        )
        .len(),
        1
    );
}

#[test]
fn upload_retries_after_refresh() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    fake.add_refresh_token("ss_rt_1");
    write_credentials(
        h.path(),
        &fake,
        stored(&fake, "ss_at_stale", "ss_rt_1", future()),
    );
    let data = pattern(200_000);
    let file = h.path().join("a.bgcode");
    std::fs::write(&file, &data).unwrap();
    let out = run(cli(&fake, h.path())
        .args(["files", "upload", "mk4", "usb"])
        .arg(&file));
    assert_eq!(out.code, 0, "{}", out.stderr);
    let puts: Vec<_> = fake
        .requests()
        .into_iter()
        .filter(|r| r.method == "PUT")
        .collect();
    assert_eq!(puts.len(), 2);
    assert_eq!(puts[0].header("authorization"), Some("Bearer ss_at_stale"));
    assert_eq!(puts[1].header("authorization"), Some("Bearer ss_at_r1"));
    assert_eq!(puts[1].body, data);
}

#[test]
fn upload_refreshes_proactively_when_expiring() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    fake.add_refresh_token("ss_rt_1");
    let soon = starstar_cli::session::now() + 100;
    write_credentials(h.path(), &fake, stored(&fake, TOKEN, "ss_rt_1", soon));
    let file = h.path().join("a.bgcode");
    std::fs::write(&file, b"abc").unwrap();
    let out = run(cli(&fake, h.path())
        .args(["files", "upload", "mk4", "usb"])
        .arg(&file));
    assert_eq!(out.code, 0, "{}", out.stderr);
    let puts: Vec<_> = fake
        .requests()
        .into_iter()
        .filter(|r| r.method == "PUT")
        .collect();
    assert_eq!(puts.len(), 1);
    assert_eq!(puts[0].header("authorization"), Some("Bearer ss_at_r1"));
}

#[test]
fn upload_missing_local_file_is_usage_error() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out =
        run(tok(&fake, h.path()).args(["files", "upload", "mk4", "usb", "/nonexistent/x.bgcode"]));
    assert_eq!(out.code, 2);
    assert!(fake.requests().is_empty());
}

#[test]
fn upload_too_large_is_reported() {
    let fake = FakeApi::start();
    fake.set(
        "PUT",
        "/api/printers/mk4/files/usb/a.bgcode",
        Canned::json(
            413,
            json!({"error": "upload is larger than the 512 MB limit"}),
        ),
    );
    let h = tempfile::tempdir().unwrap();
    let file = h.path().join("a.bgcode");
    std::fs::write(&file, b"abc").unwrap();
    let out = run(tok(&fake, h.path())
        .args(["files", "upload", "mk4", "usb"])
        .arg(&file));
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("512 MB limit"), "{}", out.stderr);
}
