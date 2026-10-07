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
            "people": [],
            "expires_on": null,
            "owner_person_id": null,
            "planned_for": null,
            "closed_at": null,
            "replaced_by": null,
            "replaces": [],
            "related": [],
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
    Promised on the daily.

#1  decision · work · 2026-10-05 · active
    Retry 5 times
    We agreed with Anna to use 5 retries. Code: `retry(max = 5)`
    project: Billing · repos: billing-api · tickets: PAY-1234 · source: meeting, Sprint planning

#3  note · learning · 2026-10-06 · active
    Backoff
    A retry should wait longer each time.
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

#[test]
fn search_without_model_warns_and_uses_keywords() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);
    let model_folder = nv_home.path().join("models").join("bge-small-en-v1.5");

    nv(&nv_home)
        .args(["search", "retries"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("#1  decision"))
        .stderr(format!(
            "nv: model not found in {}: keyword search only\n",
            model_folder.display()
        ));
}

#[test]
fn filter_only_search_needs_no_model_and_prints_no_warning() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);
    add_retry_note_again(&nv_home);

    nv(&nv_home)
        .args(["search", "--ticket", "PAY-1234"])
        .assert()
        .success()
        .stdout(
            predicate::str::starts_with("#1  decision").and(predicate::str::contains("#2").not()),
        )
        .stderr("");

    nv(&nv_home)
        .args(["search", "--since", "2026-10-06", "--area", "work"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("#2  note"))
        .stderr("");
}

#[test]
fn search_without_query_or_filter_is_a_usage_error() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home).arg("search").assert().code(2);
    nv(&nv_home).args(["search", "--all"]).assert().code(2);
    nv(&nv_home)
        .args(["search", "--since", "yesterday"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "'yesterday' is not a date like 2026-10-08",
        ));
}

#[test]
fn expired_note_is_hidden_unless_all() {
    let nv_home = TempDir::new().unwrap();
    nv(&nv_home)
        .env("NV_NOW", MONDAY)
        .args(["add", "--title", "Anna is on vacation", "--area", "work"])
        .args(["--type", "fact", "--expires-on", "2026-10-05"])
        .write_stdin("Back on Tuesday.")
        .assert()
        .success();

    // Still true on its last day.
    nv(&nv_home)
        .env("NV_NOW", MONDAY)
        .args(["search", "--type", "fact"])
        .assert()
        .stdout(predicate::str::contains("expires: 2026-10-05"));
    nv(&nv_home)
        .args(["search", "--type", "fact"])
        .assert()
        .success()
        .stdout("No notes found.\n");
    nv(&nv_home)
        .args(["search", "--type", "fact", "--all"])
        .assert()
        .stdout(predicate::str::starts_with("#1  fact"));
    // Still shown by ID.
    nv(&nv_home).args(["note", "show", "1"]).assert().success();
}

#[test]
fn add_returns_at_once_when_model_is_missing() {
    let nv_home = TempDir::new().unwrap();

    add_retry_note(&nv_home);

    nv(&nv_home)
        .args(["model", "info"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("(missing)")
                .and(predicate::str::contains("notes: 0 embedded, 1 pending")),
        );
}

#[test]
fn reindex_without_model_fails() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home)
        .args(["model", "reindex"])
        .assert()
        .code(1)
        .stderr(predicate::str::starts_with("nv: model not found in "));
}

#[test]
fn embed_pending_is_hidden_from_help() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home)
        .args(["model", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("reindex")
                .and(predicate::str::contains("embed-pending").not()),
        );
}

// ----- commitments -----

const WEDNESDAY: &str = "2026-10-07T09:00:00+02:00";

/// Person #1 Anna Nowak and person #2 Piotr Zielinski.
fn add_people(nv_home: &TempDir) {
    nv(nv_home)
        .args(["people", "add", "Anna Nowak", "--role", "QA lead"])
        .args(["--alias", "Anna", "--alias", "anna.nowak@contoso.com"])
        .assert()
        .success()
        .stdout("Added person #1\n");
    nv(nv_home)
        .args(["people", "add", "Piotr Zielinski"])
        .assert()
        .success()
        .stdout("Added person #2\n");
}

fn add_commitment(nv_home: &TempDir, title: &str, flags: &[&str]) {
    nv(nv_home)
        .env("NV_NOW", MONDAY)
        .args([
            "add",
            "--title",
            title,
            "--area",
            "work",
            "--type",
            "commitment",
        ])
        .args(flags)
        .write_stdin("Promised on the daily.")
        .assert()
        .success();
}

/// #1 mine today, #2 mine overdue, #3 Anna's, #4 Piotr's undated, #5 and #6 mine undated.
fn add_commitments(nv_home: &TempDir) {
    add_people(nv_home);
    add_commitment(
        nv_home,
        "Send retry numbers to Anna",
        &["--planned", "2026-10-07"],
    );
    add_commitment(nv_home, "Book the exam slot", &["--planned", "2026-10-05"]);
    add_commitment(
        nv_home,
        "Review the retry PR",
        &["--owner", "1", "--planned", "2026-10-08"],
    );
    add_commitment(nv_home, "Send the staging access steps", &["--owner", "2"]);
    add_commitment(nv_home, "Read the SQLite book", &[]);
    add_commitment(nv_home, "Clean the backlog", &[]);
}

fn nv_on_wednesday(nv_home: &TempDir) -> Command {
    let mut command = nv(nv_home);
    command.env("NV_NOW", WEDNESDAY);
    command
}

#[test]
fn today_output_matches_golden_text() {
    let nv_home = TempDir::new().unwrap();
    add_commitments(&nv_home);

    nv_on_wednesday(&nv_home)
        .arg("today")
        .assert()
        .success()
        .stdout(
            "\
Planned for today (2026-10-07)
#2  Book the exam slot · planned 2026-10-05, overdue
#1  Send retry numbers to Anna

Others owe you
#3  Anna Nowak: Review the retry PR · planned 2026-10-08
#4  Piotr Zielinski: Send the staging access steps

2 more of yours have no date: nv search --type commitment --status todo
",
        );
}

#[test]
fn today_shortcut_equals_commitment_today() {
    let nv_home = TempDir::new().unwrap();
    add_commitments(&nv_home);

    let long = nv_on_wednesday(&nv_home)
        .args(["commitment", "today"])
        .output()
        .unwrap();
    let short = nv_on_wednesday(&nv_home).arg("today").output().unwrap();

    assert!(long.status.success());
    assert_eq!(long.stdout, short.stdout);
}

#[test]
fn today_with_nothing_planned_says_so() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);

    nv_on_wednesday(&nv_home)
        .arg("today")
        .assert()
        .success()
        .stdout("Nothing planned for today.\n")
        .stderr("");
}

#[test]
fn today_json_has_mine_owed_and_undated() {
    let nv_home = TempDir::new().unwrap();
    add_commitments(&nv_home);

    let view = stdout_json(nv_on_wednesday(&nv_home).args(["today", "--json"]));

    assert_eq!(view["today"], "2026-10-07");
    assert_eq!(view["undated"], 2);
    let mine = view["mine"].as_array().unwrap();
    assert_eq!(mine.len(), 2);
    assert_eq!(mine[0]["id"], 2);
    assert_eq!(mine[0]["planned_for"], "2026-10-05");
    assert_eq!(mine[0]["commitment_status"], "todo");
    assert_eq!(mine[0]["owner_person_id"], Value::Null);
    let owed = view["owed"].as_array().unwrap();
    assert_eq!(owed[0]["id"], 3);
    assert_eq!(owed[0]["owner_name"], "Anna Nowak");
    assert_eq!(owed[0]["owner_person_id"], 1);
}

#[test]
fn done_then_postpone_fails() {
    let nv_home = TempDir::new().unwrap();
    add_commitments(&nv_home);

    nv_on_wednesday(&nv_home)
        .args(["commitment", "done", "1"])
        .assert()
        .success()
        .stdout("Done #1\n");

    nv_on_wednesday(&nv_home)
        .args(["commitment", "postpone", "1", "2026-10-09"])
        .assert()
        .code(1)
        .stdout("")
        .stderr("nv: commitment #1 is already done\n");
    nv_on_wednesday(&nv_home)
        .args(["commitment", "drop", "1"])
        .assert()
        .code(1)
        .stderr("nv: commitment #1 is already done\n");

    let note = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    assert_eq!(note["commitment_status"], "done");
    assert_eq!(note["closed_at"], WEDNESDAY);
    assert_eq!(note["planned_for"], "2026-10-07");
    // Done commitments stay searchable, shown with their status.
    nv(&nv_home)
        .args(["search", "--type", "commitment", "--planned", "2026-10-07"])
        .assert()
        .stdout(predicate::str::starts_with(
            "#1  commitment · work · 2026-10-05 · done\n",
        ));
}

#[test]
fn drop_takes_commitment_out_of_today() {
    let nv_home = TempDir::new().unwrap();
    add_commitments(&nv_home);

    nv_on_wednesday(&nv_home)
        .args(["commitment", "drop", "3"])
        .assert()
        .success()
        .stdout("Dropped #3\n");

    nv_on_wednesday(&nv_home)
        .arg("today")
        .assert()
        .stdout(predicate::str::contains("Review the retry PR").not());
}

#[test]
fn done_on_a_fact_fails() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);

    nv(&nv_home)
        .args(["commitment", "done", "1"])
        .assert()
        .code(1)
        .stderr("nv: note #1 is not a commitment\n");
    nv(&nv_home)
        .args(["commitment", "done", "99"])
        .assert()
        .code(1)
        .stderr("nv: note #99 not found\n");
}

#[test]
fn postpone_moves_commitment_out_of_today() {
    let nv_home = TempDir::new().unwrap();
    add_commitments(&nv_home);

    nv_on_wednesday(&nv_home)
        .args(["commitment", "postpone", "1", "2026-10-09"])
        .assert()
        .success()
        .stdout("Postponed #1 to Fri 2026-10-09\n");

    nv_on_wednesday(&nv_home)
        .arg("today")
        .assert()
        .stdout(predicate::str::contains("Send retry numbers to Anna").not());
    nv_on_wednesday(&nv_home)
        .env("NV_NOW", "2026-10-09T08:00:00+02:00")
        .arg("today")
        .assert()
        .stdout(predicate::str::contains("#1  Send retry numbers to Anna\n"));
    nv(&nv_home)
        .args(["commitment", "postpone", "1", "next week"])
        .assert()
        .code(2);
}

#[test]
fn planned_filter_finds_commitment_by_date() {
    let nv_home = TempDir::new().unwrap();
    add_commitments(&nv_home);

    // "What did I promise to do tomorrow": the spike's only miss is a filter question.
    nv_on_wednesday(&nv_home)
        .args(["search", "--planned", "2026-10-08"])
        .assert()
        .success()
        .stdout(
            "\
#3  commitment · work · 2026-10-05 · todo
    Review the retry PR
    Promised on the daily.
    owner: Anna Nowak · planned: 2026-10-08
",
        );
}

#[test]
fn planned_for_without_commitment_type_fails() {
    let nv_home = TempDir::new().unwrap();
    add_people(&nv_home);

    nv(&nv_home)
        .args([
            "add", "--title", "Exam", "--area", "learning", "--type", "fact",
        ])
        .args(["--planned", "2026-10-08"])
        .write_stdin("The exam is on Thursday.")
        .assert()
        .code(1)
        .stderr("nv: only a commitment has a planned date: use --type commitment\n");
    nv(&nv_home)
        .args([
            "add", "--title", "Exam", "--area", "learning", "--owner", "1",
        ])
        .write_stdin("Body.")
        .assert()
        .code(1)
        .stderr("nv: only a commitment has an owner: use --type commitment\n");
    nv(&nv_home)
        .args([
            "add",
            "--title",
            "Exam",
            "--area",
            "learning",
            "--type",
            "commitment",
        ])
        .args(["--owner", "8"])
        .write_stdin("Body.")
        .assert()
        .code(1)
        .stderr("nv: person #8 not found\n");
}

#[test]
fn search_and_show_display_owner_and_planned_date() {
    let nv_home = TempDir::new().unwrap();
    add_commitments(&nv_home);

    nv(&nv_home)
        .args(["note", "show", "3"])
        .assert()
        .success()
        .stdout(
            "\
#3  commitment · work · 2026-10-05 · todo
    Review the retry PR
    owner: Anna Nowak · planned: 2026-10-08

Promised on the daily.
",
        );
    nv(&nv_home)
        .args(["note", "edit", "3", "--owner", "2"])
        .assert()
        .success();
    nv(&nv_home)
        .args(["note", "show", "3"])
        .assert()
        .stdout(predicate::str::contains(
            "owner: Piotr Zielinski · planned: 2026-10-08",
        ));
}

#[test]
fn done_drop_and_postpone_are_in_the_change_log() {
    let nv_home = TempDir::new().unwrap();
    add_commitments(&nv_home);
    nv(&nv_home)
        .args(["commitment", "postpone", "1", "2026-10-09"])
        .assert()
        .success();
    nv(&nv_home)
        .args(["commitment", "done", "1"])
        .assert()
        .success();
    nv(&nv_home)
        .args(["commitment", "drop", "2"])
        .assert()
        .success();

    let conn = rusqlite::Connection::open(nv_home.path().join("nv.db")).unwrap();
    let mut statement = conn
        .prepare(
            "SELECT action, note_id, json_extract(before_json, '$.planned_for')
             FROM change_log WHERE action IN ('done', 'drop', 'postpone') ORDER BY id",
        )
        .unwrap();
    let log: Vec<(String, i64, Option<String>)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();

    assert_eq!(
        log,
        [
            ("postpone".to_string(), 1, Some("2026-10-07".to_string())),
            ("done".to_string(), 1, Some("2026-10-09".to_string())),
            ("drop".to_string(), 2, Some("2026-10-05".to_string())),
        ]
    );
}

// ----- people -----

#[test]
fn people_add_then_list_matches_golden_text() {
    let nv_home = TempDir::new().unwrap();
    add_people(&nv_home);

    nv(&nv_home)
        .args(["people", "list"])
        .assert()
        .success()
        .stdout(
            "\
#1  Anna Nowak · QA lead
    aliases: Anna, anna.nowak@contoso.com
#2  Piotr Zielinski
",
        );
}

#[test]
fn people_search_output_matches_golden_text() {
    let nv_home = TempDir::new().unwrap();
    add_people(&nv_home);
    nv(&nv_home)
        .args([
            "people",
            "add",
            "Anna Kowalska",
            "--role",
            "backend developer",
        ])
        .args(["--alias", "Anna", "--alias", "Ania"])
        .assert()
        .success();

    // Both Annas, so Claude sees which one before it links or merges.
    nv(&nv_home)
        .args(["people", "search", "anna"])
        .assert()
        .success()
        .stdout(
            "\
#3  Anna Kowalska · backend developer
    aliases: Anna, Ania
#1  Anna Nowak · QA lead
    aliases: Anna, anna.nowak@contoso.com
",
        );
}

#[test]
fn people_search_json_has_aliases_and_role() {
    let nv_home = TempDir::new().unwrap();
    add_people(&nv_home);

    let found = stdout_json(nv(&nv_home).args(["people", "search", "nowak", "--json"]));

    assert_eq!(
        found,
        json!([{
            "id": 1,
            "name": "Anna Nowak",
            "role": "QA lead",
            "aliases": ["Anna", "anna.nowak@contoso.com"],
        }])
    );
}

#[test]
fn people_search_without_match_says_no_people_found() {
    let nv_home = TempDir::new().unwrap();
    add_people(&nv_home);

    nv(&nv_home)
        .args(["people", "search", "Katarzyna"])
        .assert()
        .success()
        .stdout("No people found.\n");
    let found = stdout_json(nv(&nv_home).args(["people", "search", "Katarzyna", "--json"]));
    assert_eq!(found, json!([]));
}

#[test]
fn alias_of_another_person_is_refused_with_their_id() {
    let nv_home = TempDir::new().unwrap();
    add_people(&nv_home);

    nv(&nv_home)
        .args(["people", "alias", "2", "anna.nowak@contoso.com"])
        .assert()
        .code(1)
        .stderr("nv: alias 'anna.nowak@contoso.com' already belongs to person #1 Anna Nowak\n");
    // A short alias can match two people.
    nv(&nv_home)
        .args(["people", "alias", "2", "Anna"])
        .assert()
        .success()
        .stdout("Added alias to person #2\n");
    nv(&nv_home)
        .args(["people", "add", "anna nowak"])
        .assert()
        .code(1)
        .stderr("nv: 'anna nowak' is already person #1 Anna Nowak\n");
}

#[test]
fn people_edit_changes_role_and_name() {
    let nv_home = TempDir::new().unwrap();
    add_people(&nv_home);

    nv(&nv_home)
        .args([
            "people",
            "edit",
            "2",
            "--name",
            "Piotr Zieliński",
            "--role",
            "DevOps",
        ])
        .assert()
        .success()
        .stdout("Edited person #2\n");

    nv(&nv_home)
        .args(["people", "search", "piotr"])
        .assert()
        .stdout(
            "\
#2  Piotr Zieliński · DevOps
    aliases: Piotr Zielinski
",
        );
    nv(&nv_home).args(["people", "edit", "2"]).assert().code(1);
    nv(&nv_home)
        .args(["people", "edit", "99", "--role", "x"])
        .assert()
        .code(1)
        .stderr("nv: person #99 not found\n");
}

fn add_note_about(nv_home: &TempDir, title: &str, people: &[&str]) {
    let mut command = nv(nv_home);
    command.args(["add", "--title", title, "--area", "work", "--type", "fact"]);
    for person in people {
        command.args(["--person", person]);
    }
    command.write_stdin("Body.").assert().success();
}

#[test]
fn add_note_with_people_shows_their_names() {
    let nv_home = TempDir::new().unwrap();
    add_people(&nv_home);
    add_note_about(&nv_home, "Anna prefers async reviews", &["1", "2"]);

    nv(&nv_home)
        .args(["note", "show", "1"])
        .assert()
        .success()
        .stdout(
            "\
#1  fact · work · 2026-10-06 · active
    Anna prefers async reviews
    people: Anna Nowak, Piotr Zielinski

Body.
",
        );
    let note = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    assert_eq!(note["people"], json!([1, 2]));

    nv(&nv_home)
        .args(["note", "edit", "1", "--person", "2"])
        .assert()
        .success();
    let note = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    assert_eq!(note["people"], json!([2]));

    nv(&nv_home)
        .args([
            "add", "--title", "Unknown", "--area", "work", "--person", "8",
        ])
        .write_stdin("Body.")
        .assert()
        .code(1)
        .stderr("nv: person #8 not found\n");
}

#[test]
fn search_by_person_finds_their_notes() {
    let nv_home = TempDir::new().unwrap();
    add_people(&nv_home);
    add_note_about(&nv_home, "Anna prefers async reviews", &["1"]);
    add_note_about(&nv_home, "Piotr owns the gateway", &["2"]);
    add_commitment(&nv_home, "Review the retry PR", &["--owner", "1"]);

    nv(&nv_home)
        .args(["search", "--person", "1"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Anna prefers async reviews")
                .and(predicate::str::contains("Review the retry PR"))
                .and(predicate::str::contains("Piotr owns the gateway").not()),
        )
        .stderr("");
}

#[test]
fn merge_moves_notes_to_the_kept_person() {
    let nv_home = TempDir::new().unwrap();
    add_people(&nv_home);
    nv(&nv_home)
        .args(["people", "add", "Anna N.", "--alias", "Ania"])
        .assert()
        .success()
        .stdout("Added person #3\n");
    add_note_about(&nv_home, "Anna prefers async reviews", &["3"]);
    add_commitment(&nv_home, "Review the retry PR", &["--owner", "3"]);

    nv(&nv_home)
        .args(["people", "merge", "1", "3"])
        .assert()
        .success()
        .stdout("Merged person #3 into #1\n");

    nv(&nv_home).args(["people", "list"]).assert().stdout(
        "\
#1  Anna Nowak · QA lead
    aliases: Anna, anna.nowak@contoso.com, Anna N., Ania
#2  Piotr Zielinski
",
    );
    nv(&nv_home)
        .args(["search", "--person", "1"])
        .assert()
        .stdout(
            predicate::str::contains("people: Anna Nowak")
                .and(predicate::str::contains("owner: Anna Nowak")),
        );
    nv(&nv_home)
        .args(["search", "--person", "3"])
        .assert()
        .stdout("No notes found.\n");
}

#[test]
fn merge_same_person_fails() {
    let nv_home = TempDir::new().unwrap();
    add_people(&nv_home);

    nv(&nv_home)
        .args(["people", "merge", "1", "1"])
        .assert()
        .code(1)
        .stderr("nv: a person cannot be merged into itself\n");
    nv(&nv_home)
        .args(["people", "merge", "1", "99"])
        .assert()
        .code(1)
        .stderr("nv: person #99 not found\n");
}

// ----- replace, links, history, undo -----

fn add_plain_note(nv_home: &TempDir, title: &str, now: &str) {
    nv(nv_home)
        .env("NV_NOW", now)
        .args([
            "add", "--title", title, "--area", "work", "--type", "decision",
        ])
        .write_stdin("Body.")
        .assert()
        .success();
}

fn replace_note(nv_home: &TempDir, old_id: &str, title: &str) {
    nv(nv_home)
        .args([
            "note", "replace", old_id, "--title", title, "--area", "work",
        ])
        .args(["--type", "decision"])
        .write_stdin("Body.")
        .assert()
        .success();
}

#[test]
fn replace_output_and_search_output_shows_outdated_arrow_to_newer_note() {
    let nv_home = TempDir::new().unwrap();
    add_plain_note(&nv_home, "Retry 3 times", MONDAY);

    nv(&nv_home)
        .args([
            "note",
            "replace",
            "1",
            "--title",
            "Retry 5 times for billing-api",
        ])
        .args(["--area", "work", "--type", "decision"])
        .write_stdin("Agreed with Anna because of timeouts.")
        .assert()
        .success()
        .stdout("Saved #2, replaces #1\n");

    nv(&nv_home)
        .args(["search", "retry"])
        .assert()
        .success()
        .stdout(
            "\
#2  decision · work · 2026-10-06 · active
    Retry 5 times for billing-api
    Agreed with Anna because of timeouts.
    replaces: #1

#1  decision · work · 2026-10-05 · outdated → #2
    Retry 3 times
    Body.
",
        );
    let old = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    assert_eq!(old["status"], "outdated");
    assert_eq!(old["replaced_by"], 2);
    let new = stdout_json(nv(&nv_home).args(["note", "show", "2", "--json"]));
    assert_eq!(new["replaces"], json!([1]));

    nv(&nv_home)
        .args([
            "note",
            "replace",
            "1",
            "--title",
            "Retry 7 times",
            "--area",
            "work",
        ])
        .write_stdin("Body.")
        .assert()
        .code(1)
        .stderr("nv: note #1 is already outdated, replaced by #2\n");
    nv(&nv_home)
        .args(["note", "delete", "2"])
        .assert()
        .code(1)
        .stderr("nv: note #2 replaced #1: undo the replace, or replace #2\n");
}

#[test]
fn show_output_lists_replaces_and_related() {
    let nv_home = TempDir::new().unwrap();
    add_plain_note(&nv_home, "Retry 3 times", MONDAY);
    add_plain_note(&nv_home, "Retry budget", MONDAY);
    add_plain_note(&nv_home, "Backoff", MONDAY);
    replace_note(&nv_home, "1", "Retry 5 times");

    nv(&nv_home)
        .args(["note", "link", "4", "2"])
        .assert()
        .success()
        .stdout("Linked #4 and #2\n");
    nv(&nv_home)
        .args(["note", "link", "3", "4"])
        .assert()
        .success();
    nv(&nv_home)
        .args(["note", "link", "2", "4"])
        .assert()
        .success()
        .stdout("#2 and #4 are already linked\n");

    nv(&nv_home)
        .args(["note", "show", "4"])
        .assert()
        .success()
        .stdout(
            "\
#4  decision · work · 2026-10-06 · active
    Retry 5 times
    replaces: #1 · related: #2, #3

Body.
",
        );
    let related = stdout_json(nv(&nv_home).args(["note", "show", "2", "--json"]));
    assert_eq!(related["related"], json!([4]));
}

#[test]
fn link_to_itself_fails() {
    let nv_home = TempDir::new().unwrap();
    add_plain_note(&nv_home, "Retry 3 times", MONDAY);

    nv(&nv_home)
        .args(["note", "link", "1", "1"])
        .assert()
        .code(1)
        .stderr("nv: a note can never be linked to itself\n");
    nv(&nv_home)
        .args(["note", "link", "1", "99"])
        .assert()
        .code(1)
        .stderr("nv: note #99 not found\n");
}

/// Changes #1 add, #2 person-add, #3 add, #4 replace, #5 edit.
fn make_history(nv_home: &TempDir) {
    add_plain_note(nv_home, "Retry 3 times", MONDAY);
    nv(nv_home)
        .env("NV_NOW", MONDAY)
        .env("CLAUDECODE", "1")
        .args(["people", "add", "Anna Nowak"])
        .assert()
        .success();
    replace_note(nv_home, "1", "Retry 5 times");
    nv(nv_home)
        .env("NV_NOW", WEDNESDAY)
        .env("CLAUDECODE", "1")
        .args([
            "note",
            "edit",
            "2",
            "--title",
            "Retry 5 times for billing-api",
        ])
        .assert()
        .success();
}

#[test]
fn history_output_matches_golden_text() {
    let nv_home = TempDir::new().unwrap();
    make_history(&nv_home);

    nv(&nv_home).arg("history").assert().success().stdout(
        "\
#5  2026-10-07 09:00  claude  edit        note #2  Retry 5 times for billing-api
#4  2026-10-06 10:30  user    replace     note #1  Retry 3 times
#3  2026-10-06 10:30  user    add         note #2  Retry 5 times for billing-api
#2  2026-10-05 09:00  claude  person-add  person #1  Anna Nowak
#1  2026-10-05 09:00  user    add         note #1  Retry 3 times
",
    );
    nv(&nv_home)
        .args(["history", "--note", "1", "--limit", "1"])
        .assert()
        .stdout("#4  2026-10-06 10:30  user  replace  note #1  Retry 3 times\n");
    nv(&nv_home)
        .args(["history", "--person", "1"])
        .assert()
        .stdout("#2  2026-10-05 09:00  claude  person-add  person #1  Anna Nowak\n");
}

#[test]
fn history_of_an_empty_database_says_so() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home)
        .arg("history")
        .assert()
        .success()
        .stdout("No changes yet.\n");
    nv(&nv_home)
        .args(["history", "undo"])
        .assert()
        .code(1)
        .stderr("nv: nothing to undo\n");
}

#[test]
fn history_json_has_action_subject_and_undone() {
    let nv_home = TempDir::new().unwrap();
    make_history(&nv_home);

    let entries = stdout_json(nv(&nv_home).args(["history", "--limit", "2", "--json"]));

    assert_eq!(
        entries,
        json!([
            {
                "id": 5, "at": WEDNESDAY, "actor": "claude", "action": "edit",
                "subject": "note", "subject_id": 2,
                "title": "Retry 5 times for billing-api", "undone": false,
            },
            {
                "id": 4, "at": TUESDAY, "actor": "user", "action": "replace",
                "subject": "note", "subject_id": 1, "title": "Retry 3 times", "undone": false,
            },
        ])
    );
}

#[test]
fn undo_last_change_then_history_shows_it_undone() {
    let nv_home = TempDir::new().unwrap();
    make_history(&nv_home);

    nv_on_wednesday(&nv_home)
        .args(["history", "undo"])
        .assert()
        .success()
        .stdout("Undone #5: edit of note #2\n")
        .stderr("");

    nv(&nv_home)
        .args(["history", "--limit", "2"])
        .assert()
        .stdout(
            "\
#6  2026-10-07 09:00  user    undo  note #2  Retry 5 times
#5  2026-10-07 09:00  claude  edit  note #2  Retry 5 times  (undone)
",
        );
    // The next undo takes the replace: the new note goes, the old one is true again.
    nv_on_wednesday(&nv_home)
        .args(["history", "undo"])
        .assert()
        .success()
        .stdout("Undone #4: replace of note #1\n");
    nv(&nv_home)
        .args(["search", "--type", "decision"])
        .assert()
        .stdout(
            "\
#1  decision · work · 2026-10-05 · active
    Retry 3 times
    Body.
",
        );
}

#[test]
fn undo_of_delete_brings_note_back_to_search() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);
    nv(&nv_home)
        .args(["note", "delete", "1"])
        .assert()
        .success();
    nv(&nv_home)
        .args(["search", "retry"])
        .assert()
        .stdout("No notes found.\n");

    nv(&nv_home)
        .args(["history", "undo", "2"])
        .assert()
        .success()
        .stdout("Undone #2: delete of note #1\n");

    nv(&nv_home)
        .args(["search", "retry"])
        .assert()
        .stdout(predicate::str::starts_with(
            "#1  decision · work · 2026-10-05 · active\n",
        ));
    let note = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    assert_eq!(note["repos"], json!(["billing-api"]));
    assert_eq!(note["tickets"], json!(["PAY-1234"]));
}

#[test]
fn undo_older_change_is_refused_with_the_blocking_change_id() {
    let nv_home = TempDir::new().unwrap();
    make_history(&nv_home);

    nv(&nv_home)
        .args(["history", "undo", "4"])
        .assert()
        .code(1)
        .stdout("")
        .stderr("nv: note #2 changed after change #4: undo change #5 first\n");
    nv(&nv_home)
        .args(["history", "undo", "3"])
        .assert()
        .code(1)
        .stderr("nv: note #2 changed after change #3: undo change #5 first\n");
    nv(&nv_home)
        .args(["history", "undo", "99"])
        .assert()
        .code(1)
        .stderr("nv: change #99 not found\n");
}

#[test]
fn undo_of_merge_warns_about_nothing_and_brings_person_back() {
    let nv_home = TempDir::new().unwrap();
    add_people(&nv_home);
    nv(&nv_home)
        .args(["people", "add", "Anna N."])
        .assert()
        .success();
    nv(&nv_home)
        .args(["people", "merge", "1", "3"])
        .assert()
        .success();

    nv(&nv_home)
        .args(["history", "undo"])
        .assert()
        .success()
        .stdout("Undone #4: merge of person #1\n");

    nv(&nv_home)
        .args(["people", "search", "Anna N."])
        .assert()
        .stdout("#3  Anna N.\n");
}

// ----- one name for the planned date, lists on edit, replace copies fields -----

#[test]
fn add_takes_planned_date() {
    let nv_home = TempDir::new().unwrap();
    add_commitment(&nv_home, "Book the exam slot", &["--planned", "2026-10-08"]);

    let note = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    assert_eq!(note["planned_for"], "2026-10-08");
}

#[test]
fn planned_for_still_works_and_is_hidden_from_help() {
    let nv_home = TempDir::new().unwrap();
    add_commitment(
        &nv_home,
        "Book the exam slot",
        &["--planned-for", "2026-10-08"],
    );

    let note = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    assert_eq!(note["planned_for"], "2026-10-08");
    nv(&nv_home)
        .args(["add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--planned <PLANNED>"))
        .stdout(predicate::str::contains("--planned-for").not());
}

#[test]
fn edit_planned_is_logged_as_edit() {
    let nv_home = TempDir::new().unwrap();
    add_commitment(&nv_home, "Book the exam slot", &["--planned", "2026-10-08"]);
    add_plain_note(&nv_home, "Retry 3 times", MONDAY);

    nv(&nv_home)
        .args(["note", "edit", "1", "--planned", "2026-10-12"])
        .assert()
        .success()
        .stdout("Edited #1, planned Mon 2026-10-12\n");

    let note = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    assert_eq!(note["planned_for"], "2026-10-12");
    nv(&nv_home)
        .args(["history", "--note", "1", "--limit", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("  edit  "));
    nv(&nv_home)
        .args(["note", "edit", "2", "--planned", "2026-10-12"])
        .assert()
        .code(1)
        .stderr("nv: only a commitment has a planned date: use --type commitment\n");
    nv(&nv_home)
        .args(["commitment", "done", "1"])
        .assert()
        .success();
    nv(&nv_home)
        .args(["note", "edit", "1", "--planned", "2026-10-13"])
        .assert()
        .code(1)
        .stderr("nv: a done commitment keeps its planned date\n");
}

#[test]
fn edit_add_person_keeps_other_people() {
    let nv_home = TempDir::new().unwrap();
    add_people(&nv_home);
    add_note_about(&nv_home, "Retry 5 times", &["1"]);

    nv(&nv_home)
        .args(["note", "edit", "1", "--add-person", "2"])
        .args(["--add-repo", "billing-api", "--add-ticket", "PAY-1234"])
        .assert()
        .success();
    let note = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    assert_eq!(note["people"], json!([1, 2]));
    assert_eq!(note["repos"], json!(["billing-api"]));
    assert_eq!(note["tickets"], json!(["PAY-1234"]));

    nv(&nv_home)
        .args(["note", "edit", "1", "--remove-person", "1"])
        .args(["--add-repo", "gateway", "--remove-ticket", "PAY-1234"])
        .assert()
        .success();
    let note = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    assert_eq!(note["people"], json!([2]));
    assert_eq!(note["repos"], json!(["billing-api", "gateway"]));
    assert_eq!(note["tickets"], json!([]));

    nv(&nv_home)
        .args(["note", "edit", "1", "--remove-repo", "ledger"])
        .assert()
        .code(1)
        .stderr("nv: note #1 has no repo ledger\n");

    // The change is one entry in the change log and can be taken back.
    nv(&nv_home).args(["history", "undo"]).assert().success();
    let note = stdout_json(nv(&nv_home).args(["note", "show", "1", "--json"]));
    assert_eq!(note["people"], json!([1, 2]));
    assert_eq!(note["repos"], json!(["billing-api"]));
}

#[test]
fn edit_repo_with_add_repo_is_a_usage_error() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);

    for (whole_list, one_more) in [
        (["--repo", "ledger"], ["--add-repo", "gateway"]),
        (["--ticket", "PAY-1"], ["--remove-ticket", "PAY-1234"]),
        (["--person", "1"], ["--add-person", "2"]),
    ] {
        nv(&nv_home)
            .args(["note", "edit", "1"])
            .args(whole_list)
            .args(one_more)
            .assert()
            .code(2)
            .stderr(predicate::str::contains("cannot be used with"));
    }
}

#[test]
fn replace_with_only_title_keeps_ticket_filter_working() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);

    nv(&nv_home)
        .args(["note", "replace", "1", "--title", "Retry 7 times"])
        .write_stdin("Retry count is 7 (was 5).")
        .assert()
        .success()
        .stdout("Saved #2, replaces #1\n");

    let newer = stdout_json(nv(&nv_home).args(["note", "show", "2", "--json"]));
    assert_eq!(newer["area"], "work");
    assert_eq!(newer["type"], "decision");
    assert_eq!(newer["project"], "Billing");
    assert_eq!(newer["repos"], json!(["billing-api"]));
    assert_eq!(newer["tickets"], json!(["PAY-1234"]));
    assert_eq!(newer["source"]["kind"], "meeting");
    let found = stdout_json(nv(&nv_home).args(["search", "--ticket", "PAY-1234", "--json"]));
    assert_eq!(found[0]["id"], 2);
    assert_eq!(found[1]["id"], 1);
}

#[test]
fn replace_flags_override_copied_fields_and_commitment_keeps_owner() {
    let nv_home = TempDir::new().unwrap();
    add_people(&nv_home);
    add_commitment(
        &nv_home,
        "Review the retry PR",
        &[
            "--owner",
            "1",
            "--planned",
            "2026-10-08",
            "--repo",
            "billing-api",
        ],
    );

    nv(&nv_home)
        .args(["note", "replace", "1", "--title", "Review both retry PRs"])
        .args(["--repo", "gateway", "--planned", "2026-10-12"])
        .write_stdin("Anna Nowak reviews both PRs.")
        .assert()
        .success();

    let newer = stdout_json(nv(&nv_home).args(["note", "show", "2", "--json"]));
    assert_eq!(newer["type"], "commitment");
    assert_eq!(newer["commitment_status"], "todo");
    assert_eq!(newer["owner_person_id"], 1);
    assert_eq!(newer["planned_for"], "2026-10-12");
    assert_eq!(newer["repos"], json!(["gateway"]));
}

// ----- search: status and project filters, the total, the first body line -----

#[test]
fn search_status_todo_hides_done_commitments() {
    let nv_home = TempDir::new().unwrap();
    add_commitments(&nv_home);
    nv(&nv_home)
        .args(["commitment", "done", "5"])
        .assert()
        .success();

    let todo =
        stdout_json(nv(&nv_home).args(["search", "--status", "todo", "--limit", "50", "--json"]));
    let done = stdout_json(nv(&nv_home).args(["search", "--status", "done", "--json"]));

    let todo_ids: Vec<i64> = todo
        .as_array()
        .unwrap()
        .iter()
        .map(|note| note["id"].as_i64().unwrap())
        .collect();
    assert_eq!(todo_ids, [6, 4, 3, 2, 1]);
    assert_eq!(done.as_array().unwrap().len(), 1);
    assert_eq!(done[0]["id"], 5);
}

#[test]
fn search_with_unknown_status_names_the_allowed_ones() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home)
        .args(["search", "--status", "open"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "unknown commitment status 'open': use todo, done, dropped",
        ));
}

#[test]
fn search_by_project_finds_its_notes() {
    let nv_home = TempDir::new().unwrap();
    add_retry_note(&nv_home);
    add_plain_note(&nv_home, "Retry 3 times", MONDAY);

    let found = stdout_json(nv(&nv_home).args(["search", "--project", "billing", "--json"]));

    assert_eq!(found.as_array().unwrap().len(), 1);
    assert_eq!(found[0]["id"], 1);
}

#[test]
fn filter_only_search_says_when_more_exist() {
    let nv_home = TempDir::new().unwrap();
    add_commitments(&nv_home);

    let output = nv(&nv_home)
        .args(["search", "--type", "commitment", "--limit", "2"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = String::from_utf8(output).unwrap();
    assert!(
        text.ends_with("    Promised on the daily.\n\nshowing 2 of 6, use --limit\n"),
        "{text}"
    );
}

#[test]
fn no_showing_line_when_all_fit_or_on_text_search() {
    let nv_home = TempDir::new().unwrap();
    add_commitments(&nv_home);

    nv(&nv_home)
        .args(["search", "--type", "commitment", "--limit", "6"])
        .assert()
        .success()
        .stdout(predicate::str::contains("showing").not());
    nv(&nv_home)
        .args(["search", "promised daily", "--limit", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("showing").not());
}

#[test]
fn search_shows_first_body_line_cut_at_100_characters() {
    let nv_home = TempDir::new().unwrap();
    let long_line = "word ".repeat(30);
    nv(&nv_home)
        .args(["add", "--title", "Long", "--area", "work"])
        .write_stdin(format!("\n\n{long_line}\nSecond line.\n"))
        .assert()
        .success();

    let first_hundred: String = long_line.chars().take(100).collect();
    nv(&nv_home)
        .args(["search", "--area", "work"])
        .assert()
        .success()
        .stdout(format!(
            "#1  note · work · 2026-10-06 · active\n    Long\n    {}…\n",
            first_hundred.trim_end()
        ));
}

#[test]
fn show_does_not_repeat_first_line() {
    let nv_home = TempDir::new().unwrap();
    add_plain_note(&nv_home, "Retry 3 times", MONDAY);

    nv(&nv_home)
        .args(["note", "show", "1"])
        .assert()
        .success()
        .stdout("#1  decision · work · 2026-10-05 · active\n    Retry 3 times\n\nBody.\n");
}

#[test]
fn add_refuses_password_written_in_a_sentence() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home)
        .args(["add", "--title", "Staging DB access", "--area", "work"])
        .write_stdin("Staging DB password is hunter2 by the way.")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("hunter2").not())
        .stderr(predicate::str::contains("nv never stores secrets"));
}

// ----- answers name the weekday; results show the start of the body -----

#[test]
fn answers_show_planned_and_expiry_dates_with_weekday() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home)
        .args(["add", "--title", "Send retry numbers", "--area", "work"])
        .args(["--type", "commitment", "--planned", "2026-10-08"])
        .write_stdin("I promised to send the retry numbers.")
        .assert()
        .success()
        .stdout("Saved #1, planned Thu 2026-10-08\n");
    nv(&nv_home)
        .args(["add", "--title", "Vacation requests", "--area", "work"])
        .args(["--expires-on", "2026-11-14"])
        .write_stdin("Enter December vacation in Workday by 2026-11-14.")
        .assert()
        .success()
        .stdout("Saved #2, expires Sat 2026-11-14\n");
    nv(&nv_home)
        .args(["note", "edit", "1", "--planned", "2026-10-12"])
        .assert()
        .success()
        .stdout("Edited #1, planned Mon 2026-10-12\n");
    nv(&nv_home)
        .args(["note", "replace", "1", "--title", "Send both retry reports"])
        .write_stdin("I promised to send both retry reports.")
        .assert()
        .success()
        .stdout("Saved #3, replaces #1, planned Mon 2026-10-12\n");
    nv(&nv_home)
        .args(["note", "delete", "2"])
        .assert()
        .success()
        .stdout("Deleted #2\n");
}

#[test]
fn search_joins_the_wrapped_lines_of_the_first_paragraph() {
    let nv_home = TempDir::new().unwrap();
    nv(&nv_home)
        .args(["add", "--title", "Review the PR", "--area", "work"])
        .write_stdin(
            "Anna promised to review the PR that\nraises the retry count.\n\nSecond paragraph.\n",
        )
        .assert()
        .success();

    nv(&nv_home)
        .args(["search", "--area", "work"])
        .assert()
        .success()
        .stdout(
            "\
#1  note · work · 2026-10-06 · active
    Review the PR
    Anna promised to review the PR that raises the retry count.
",
        );
}

// ----- nv date and the weekday check -----

#[test]
fn date_lists_today_and_the_next_14_days_with_weekdays() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home).arg("date").assert().success().stdout(
        "\
Tue 2026-10-06  today
Wed 2026-10-07  tomorrow
Thu 2026-10-08
Fri 2026-10-09
Sat 2026-10-10
Sun 2026-10-11
Mon 2026-10-12
Tue 2026-10-13
Wed 2026-10-14
Thu 2026-10-15
Fri 2026-10-16
Sat 2026-10-17
Sun 2026-10-18
Mon 2026-10-19
Tue 2026-10-20
",
    );
}

#[test]
fn date_days_sets_the_length_and_is_limited() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home)
        .args(["date", "--days", "1"])
        .assert()
        .success()
        .stdout("Tue 2026-10-06  today\nWed 2026-10-07  tomorrow\n");
    nv(&nv_home)
        .args(["date", "--days", "0"])
        .assert()
        .success()
        .stdout("Tue 2026-10-06  today\n");
    nv(&nv_home)
        .args(["date", "--days", "400"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--days"));
}

#[test]
fn date_json_has_date_weekday_and_label() {
    let nv_home = TempDir::new().unwrap();

    let days = stdout_json(nv(&nv_home).args(["date", "--days", "2", "--json"]));

    assert_eq!(
        days,
        json!([
            {"date": "2026-10-06", "weekday": "Tuesday", "label": "today"},
            {"date": "2026-10-07", "weekday": "Wednesday", "label": "tomorrow"},
            {"date": "2026-10-08", "weekday": "Thursday", "label": null},
        ])
    );
}

#[test]
fn date_does_not_need_the_model_or_write_to_the_change_log() {
    let nv_home = TempDir::new().unwrap();

    nv(&nv_home).arg("date").assert().success();

    nv(&nv_home)
        .arg("history")
        .assert()
        .success()
        .stdout("No changes yet.\n");
}

#[test]
fn add_edit_and_replace_refuse_a_weekday_that_does_not_match_its_date() {
    let nv_home = TempDir::new().unwrap();
    let refusal = "nv: the body says \"Friday 2026-10-10\", but 2026-10-10 is a Saturday: \
                   check the date (`nv date` lists the next days)\n";

    nv(&nv_home)
        .args(["add", "--title", "Review the PR", "--area", "work"])
        .write_stdin("Anna reviews the PR by Friday 2026-10-10.")
        .assert()
        .code(1)
        .stderr(refusal);
    nv(&nv_home)
        .arg("history")
        .assert()
        .stdout("No changes yet.\n");

    add_plain_note(&nv_home, "Retry 3 times", MONDAY);
    nv(&nv_home)
        .args(["note", "edit", "1", "--body"])
        .write_stdin("Reviewed by Friday 2026-10-10.")
        .assert()
        .code(1)
        .stderr(refusal);
    nv(&nv_home)
        .args(["note", "replace", "1", "--title", "Retry 5 times"])
        .write_stdin("Reviewed by Friday 2026-10-10.")
        .assert()
        .code(1)
        .stderr(refusal);

    nv(&nv_home)
        .args(["add", "--title", "Review the PR", "--area", "work"])
        .write_stdin("Anna reviews the PR by Friday 2026-10-09.")
        .assert()
        .success();
}
