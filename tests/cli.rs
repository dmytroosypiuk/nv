use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::{Value, json};
use tempfile::TempDir;

const MONDAY: &str = "2026-10-05T09:00:00+02:00";
const TUESDAY: &str = "2026-10-06T10:30:00+02:00";

/// `nv` with a temporary `NV_HOME` and a fixed clock, so tests never touch the real one.
fn nv(nv_home: &TempDir) -> Command {
    let mut command = cargo_bin_cmd!("nv");
    command
        .env("NV_HOME", nv_home.path())
        .env("NV_NOW", TUESDAY)
        .env_remove("NV_ACTOR")
        .env_remove("CLAUDECODE");
    command
}

/// Saves the retry decision from the design doc as note #1, on Monday.
fn add_retry_note(nv_home: &TempDir) {
    nv(nv_home)
        .env("NV_NOW", MONDAY)
        .args([
            "add",
            "--title",
            "Retry 5 times",
            "--area",
            "work",
            "--type",
            "decision",
        ])
        .args([
            "--project",
            "Billing",
            "--repo",
            "billing-api",
            "--ticket",
            "PAY-1234",
        ])
        .args([
            "--source-kind",
            "meeting",
            "--source-ref",
            "Sprint planning",
        ])
        .write_stdin("We agreed with Anna to use 5 retries.\nCode: `retry(max = 5)`\n")
        .assert()
        .success()
        .stdout("Saved #1\n");
}

fn stdout_json(command: &mut Command) -> Value {
    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).unwrap()
}

#[test]
fn version_flag_prints_name_and_version() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home)
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("nv {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn add_reads_body_from_stdin_with_quotes_and_dollar_signs() {
    let nv_home = TempDir::new().unwrap();
    let body = "Run `echo \"$HOME\"` and 'single' quotes; costs $5 && more\n\tindented";

    nv(&nv_home)
        .args([
            "note",
            "add",
            "--title",
            "Shell quoting",
            "--area",
            "learning",
        ])
        .write_stdin(body)
        .assert()
        .success()
        .stdout("Saved #1\n");

    let note = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    assert_eq!(note["body"], body);
}

#[test]
fn add_shortcut_equals_note_add() {
    let nv_home = TempDir::new().unwrap();
    for command in [&["add"][..], &["note", "add"]] {
        nv(&nv_home)
            .args(command)
            .args(["--title", "Same note", "--area", "personal"])
            .write_stdin("Body.")
            .assert()
            .success();
    }

    let mut first = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    let mut second = stdout_json(nv(&nv_home).args(["note", "show", "2", "--json"]));
    first["id"].take();
    second["id"].take();

    assert_eq!(first, second);
}

#[test]
fn add_without_area_is_a_usage_error() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home)
        .args(["add", "--title", "No area"])
        .write_stdin("Body.")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--area"));
}

#[test]
fn add_with_unknown_area_names_the_allowed_ones() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home)
        .args(["add", "--title", "Hobby", "--area", "hobby"])
        .write_stdin("Body.")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("use work, learning, personal"));
}

#[test]
fn add_without_body_fails() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home)
        .args(["add", "--title", "Empty", "--area", "work"])
        .write_stdin("\n")
        .assert()
        .code(1)
        .stderr("nv: a note needs a body: give it on stdin\n");
}

#[test]
fn add_refuses_text_that_looks_like_a_token() {
    let nv_home = TempDir::new().unwrap();
    for secret in [
        "ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8",
        "sk-proj-abcdefghijklmnopqrstuvwx",
        "password=hunter2",
    ] {
        nv(&nv_home)
            .args(["add", "--title", "Staging access", "--area", "work"])
            .write_stdin(format!("Use {secret} to log in."))
            .assert()
            .code(1)
            .stdout("")
            .stderr(
                predicate::str::contains("nv never stores secrets")
                    .and(predicate::str::contains(secret).not()),
            );
    }

    nv(&nv_home)
        .args(["note", "show", "1"])
        .assert()
        .code(1)
        .stderr("nv: note #1 not found\n");
}

#[test]
fn show_output_matches_golden_text() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);

    nv(&nv_home)
        .args(["note", "show", "1"])
        .assert()
        .success()
        .stdout(
            "\
#1  decision · work · 2026-10-05 · active
    Retry 5 times
    project: Billing · repos: billing-api · tickets: PAY-1234 · source: meeting, Sprint planning

We agreed with Anna to use 5 retries.
Code: `retry(max = 5)`
",
        );
}

#[test]
fn show_json_has_all_fields() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);

    let note = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));

    assert_eq!(
        note,
        json!({
            "id": 1,
            "title": "Retry 5 times",
            "body": "We agreed with Anna to use 5 retries.\nCode: `retry(max = 5)`",
            "area": "work",
            "type": "decision",
            "project": "Billing",
            "status": "active",
            "commitment_status": null,
            "source": {"kind": "meeting", "ref": "Sprint planning"},
            "repos": ["billing-api"],
            "tickets": ["PAY-1234"],
            "created_at": MONDAY,
            "updated_at": MONDAY,
        })
    );
}

#[test]
fn show_missing_note_exits_with_error() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home)
        .args(["note", "show", "99"])
        .assert()
        .code(1)
        .stdout("")
        .stderr("nv: note #99 not found\n");
}

#[test]
fn edit_changes_title_and_keeps_body() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);

    nv(&nv_home)
        .args([
            "note",
            "edit",
            "1",
            "--title",
            "Retry 5 times for billing-api",
        ])
        .assert()
        .success()
        .stdout("Edited #1\n");

    let note = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    assert_eq!(note["title"], "Retry 5 times for billing-api");
    assert_eq!(
        note["body"],
        "We agreed with Anna to use 5 retries.\nCode: `retry(max = 5)`"
    );
    assert_eq!(note["created_at"], MONDAY);
    assert_eq!(note["updated_at"], TUESDAY);
}

#[test]
fn edit_reads_new_body_from_stdin_only_with_body_flag() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);

    nv(&nv_home)
        .args([
            "note", "edit", "1", "--body", "--ticket", "PAY-1", "--ticket", "PAY-2",
        ])
        .write_stdin("Now 7 retries.\n")
        .assert()
        .success();

    let note = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    assert_eq!(note["body"], "Now 7 retries.");
    assert_eq!(note["tickets"], json!(["PAY-1", "PAY-2"]));
    assert_eq!(note["repos"], json!(["billing-api"]));
}

#[test]
fn edit_without_fields_fails() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);

    nv(&nv_home)
        .args(["note", "edit", "1"])
        .assert()
        .code(1)
        .stderr("nv: nothing to change: give at least one field\n");
}

#[test]
fn delete_then_show_fails() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);

    nv(&nv_home)
        .args(["note", "delete", "1"])
        .assert()
        .success()
        .stdout("Deleted #1\n");

    nv(&nv_home).args(["note", "show", "1"]).assert().code(1);
    nv(&nv_home).args(["note", "delete", "1"]).assert().code(1);
}

#[test]
fn search_output_matches_golden_text() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);
    nv(&nv_home)
        .args([
            "add",
            "--title",
            "Send retry numbers to Anna",
            "--area",
            "work",
        ])
        .args(["--type", "commitment"])
        .write_stdin("Promised on the daily.")
        .assert()
        .success();
    nv(&nv_home)
        .args(["add", "--title", "Backoff", "--area", "learning"])
        .write_stdin("A retry should wait longer each time.")
        .assert()
        .success();
    nv(&nv_home)
        .args(["add", "--title", "Staging postgres", "--area", "work"])
        .write_stdin("Connect through the bastion host.")
        .assert()
        .success();

    nv(&nv_home)
        .args(["search", "retries"])
        .assert()
        .success()
        .stdout(
            "\
#2  commitment · work · 2026-10-06 · todo
    Send retry numbers to Anna

#1  decision · work · 2026-10-05 · active
    Retry 5 times
    project: Billing · repos: billing-api · tickets: PAY-1234 · source: meeting, Sprint planning

#3  note · learning · 2026-10-06 · active
    Backoff
",
        );
}

#[test]
fn search_shortcut_equals_note_search_and_respects_limit() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);
    add_retry_note_again(&nv_home);

    let long = nv(&nv_home)
        .args(["note", "search", "retry"])
        .output()
        .unwrap();
    let short = nv(&nv_home).args(["search", "retry"]).output().unwrap();
    assert_eq!(long.stdout, short.stdout);

    let limited = stdout_json(nv(&nv_home).args(["search", "retry", "--limit", "1", "--json"]));
    assert_eq!(limited.as_array().unwrap().len(), 1);
}

fn add_retry_note_again(nv_home: &TempDir) {
    nv(nv_home)
        .args(["add", "--title", "Retry budget", "--area", "work"])
        .write_stdin("At most 10% of calls may be retries.")
        .assert()
        .success();
}

#[test]
fn search_json_is_a_list_of_notes_with_rank() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);
    add_retry_note_again(&nv_home);

    let found = stdout_json(nv(&nv_home).args(["search", "retry", "--json"]));

    let found = found.as_array().unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0]["rank"], 1);
    assert_eq!(found[1]["rank"], 2);
    assert_eq!(found[0]["title"], "Retry budget");
    assert_eq!(found[1]["id"], 1);
    assert_eq!(found[1]["repos"], json!(["billing-api"]));
}

#[test]
fn search_with_no_match_says_no_notes_found() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);

    nv(&nv_home)
        .args(["search", "kubernetes"])
        .assert()
        .success()
        .stdout("No notes found.\n");

    let found = stdout_json(nv(&nv_home).args(["search", "kubernetes", "--json"]));
    assert_eq!(found, json!([]));
}

fn actors_in_change_log(nv_home: &TempDir) -> Vec<String> {
    let conn = rusqlite::Connection::open(nv_home.path().join("nv.db")).unwrap();
    let mut statement = conn
        .prepare("SELECT actor FROM change_log ORDER BY id")
        .unwrap();
    let actors = statement.query_map([], |row| row.get(0)).unwrap();
    actors.map(Result::unwrap).collect()
}

#[test]
fn actor_is_claude_when_run_by_claude_code() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);
    nv(&nv_home)
        .env("CLAUDECODE", "1")
        .args(["note", "edit", "1", "--project", "Payments"])
        .assert()
        .success();
    nv(&nv_home)
        .env("CLAUDECODE", "1")
        .env("NV_ACTOR", "user")
        .args(["note", "delete", "1"])
        .assert()
        .success();

    assert_eq!(actors_in_change_log(&nv_home), ["user", "claude", "user"]);
}

#[test]
fn unknown_nv_actor_is_an_error() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home)
        .env("NV_ACTOR", "robot")
        .args(["note", "show", "1"])
        .assert()
        .code(1)
        .stderr("nv: unknown actor 'robot': use claude, user\n");
}
