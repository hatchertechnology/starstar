mod support;

use std::io::Write;
use std::process::Stdio;

use serde_json::json;
use support::*;

fn tok(fake: &FakeApi, home: &std::path::Path) -> std::process::Command {
    let mut c = cli(fake, home);
    c.env("STARSTAR_TOKEN", TOKEN);
    c
}

fn with_stdin(mut cmd: std::process::Command, input: &str) -> Out {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let o = child.wait_with_output().unwrap();
    Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

#[test]
fn user_add_reads_password_from_stdin() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let mut cmd = tok(&fake, h.path());
    cmd.args([
        "--json",
        "user",
        "add",
        "op",
        "--role",
        "operator",
        "--password-stdin",
    ]);
    let out = with_stdin(cmd, "pw-op-12345\n");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        fake.find("POST", "/api/users")[0].json(),
        json!({"username": "op", "password": "pw-op-12345", "role": "operator"})
    );
    assert!(!out.stdout.contains("pw-op-12345") && !out.stderr.contains("pw-op-12345"));
}

#[test]
fn user_add_rejects_bad_username_and_short_password() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let mut bad = tok(&fake, h.path());
    bad.args(["user", "add", "Bad", "--role", "viewer", "--password-stdin"]);
    assert_eq!(with_stdin(bad, "pw-long-1234\n").code, 2);
    let mut short = tok(&fake, h.path());
    short.args(["user", "add", "op", "--role", "viewer", "--password-stdin"]);
    assert_eq!(with_stdin(short, "short\n").code, 2);
    assert!(fake.requests().is_empty());
}

#[test]
fn user_list_json() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["--json", "user", "list"]));
    assert_eq!(out.json()[1]["role"], "operator");
}

#[test]
fn set_role_disable_enable_send_one_field() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    for args in [
        ["set-role", "op", "viewer"],
        ["disable", "op", ""],
        ["enable", "op", ""],
    ] {
        let args: Vec<_> = args.into_iter().filter(|a| !a.is_empty()).collect();
        let out = run(tok(&fake, h.path()).arg("user").args(args));
        assert_eq!(out.code, 0, "{}", out.stderr);
    }
    let bodies: Vec<_> = fake
        .find("PATCH", "/api/users/op")
        .iter()
        .map(|r| r.json())
        .collect();
    assert_eq!(
        bodies,
        [
            json!({"role": "viewer"}),
            json!({"disabled": true}),
            json!({"disabled": false})
        ]
    );
}

#[test]
fn user_remove_needs_yes() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["user", "remove", "op"]));
    assert_eq!(out.code, 5);
    assert!(fake.requests().is_empty());
    let out = run(tok(&fake, h.path()).args(["user", "remove", "op", "--yes"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(fake.find("DELETE", "/api/users/op").len(), 1);
}

#[test]
fn passwd_clears_stored_sign_in() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    write_credentials(h.path(), &fake, stored(&fake, TOKEN, "ss_rt_1", future()));
    let mut cmd = cli(&fake, h.path());
    cmd.args(["user", "passwd", "--password-stdin"]);
    let out = with_stdin(cmd, "old-pw-123\nnew-pw-12345\n");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        fake.find("PUT", "/api/me/password")[0].json(),
        json!({"current_password": "old-pw-123", "new_password": "new-pw-12345"})
    );
    assert!(read_credentials(h.path(), &fake).is_none());
    assert!(out.stdout.contains("starstar login"));
}

#[test]
fn passwd_with_env_token_leaves_the_file_alone() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    write_credentials(
        h.path(),
        &fake,
        stored(&fake, "ss_at_o", "ss_rt_1", future()),
    );
    let mut cmd = tok(&fake, h.path());
    cmd.args(["user", "passwd", "--password-stdin"]);
    let out = with_stdin(cmd, "old-pw-123\nnew-pw-12345\n");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(read_credentials(h.path(), &fake).is_some());
    assert!(out.stdout.contains("STARSTAR_TOKEN"));
}

#[test]
fn passwd_wrong_current_is_exit_4() {
    let fake = FakeApi::start();
    fake.set(
        "PUT",
        "/api/me/password",
        Canned::json(403, json!({"error": "current password is incorrect"})),
    );
    let h = tempfile::tempdir().unwrap();
    write_credentials(h.path(), &fake, stored(&fake, TOKEN, "ss_rt_1", future()));
    let mut cmd = cli(&fake, h.path());
    cmd.args(["user", "passwd", "--password-stdin"]);
    let out = with_stdin(cmd, "wrong-pw-123\nnew-pw-12345\n");
    assert_eq!(out.code, 4);
    assert!(read_credentials(h.path(), &fake).is_some());
}

#[test]
fn apps_list_and_all() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["--json", "apps", "list"]));
    assert_eq!(out.json()[0]["client_id"], "starstar-cli");
    let human = run(tok(&fake, h.path()).args(["apps", "list", "--all"]));
    assert!(human.stdout.contains("2027-01-15"), "{}", human.stdout);
    assert_eq!(fake.find("GET", "/api/apps?all=true").len(), 1);
}

#[test]
fn apps_revoke() {
    let fake = FakeApi::start();
    let h = tempfile::tempdir().unwrap();
    let out = run(tok(&fake, h.path()).args(["--json", "apps", "revoke", "ss_gr_1"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.json()["action"], "revoke");
    assert_eq!(fake.find("DELETE", "/api/apps/ss_gr_1").len(), 1);
}
