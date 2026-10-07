//! The plugin's `SessionStart` hook: it makes sure nv is installed and tells Claude, in
//! the context of every session, that nv exists and which skill to use for what. A hook
//! that fails must never stop a session, and its stdout must be one JSON document.

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::{World, root, text};
use serde_json::Value;

/// The hook's answer: the JSON on stdout, and the context text inside it.
fn context_of(output: &std::process::Output) -> String {
    let answer: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {}", text(&output.stdout)));
    assert_eq!(
        answer["hookSpecificOutput"]["hookEventName"],
        "SessionStart"
    );
    answer["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext is text")
        .to_string()
}

fn trigger_text() -> String {
    fs::read_to_string(root().join("plugin/scripts/context.txt")).unwrap()
}

#[test]
fn it_answers_with_the_trigger_text_when_nv_is_ready() {
    let world = World::new();
    assert!(world.run("scripts/ensure-nv.sh", &[]).status.success());
    world.unpublish_release();

    let output = world.run("scripts/session-start.sh", &[]);

    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(context_of(&output), trigger_text().trim_end());
    assert_eq!(
        text(&output.stderr),
        "",
        "nothing to say when all is in place"
    );
}

#[test]
fn it_fetches_what_is_missing_and_stdout_stays_one_json_document() {
    let world = World::new();

    let output = world.run("scripts/session-start.sh", &[]);

    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(context_of(&output), trigger_text().trim_end());
    assert!(world.binary().is_file());
    assert!(world.model().is_file());
    let stdout = text(&output.stdout);
    assert_eq!(stdout.trim_end().lines().count(), 1, "one line: {stdout}");
}

#[test]
fn it_never_fails_the_session_when_the_download_is_off_and_says_what_to_do() {
    let world = World::new();

    let output = world
        .command("scripts/session-start.sh")
        .env("NV_NO_DOWNLOAD", "1")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let context = context_of(&output);
    assert!(context.contains("not installed"), "{context}");
    assert!(context.contains("NV_NO_DOWNLOAD"), "{context}");
    assert!(
        context.contains("one short line"),
        "tell the user once: {context}"
    );
}

#[test]
fn it_never_fails_the_session_when_the_platform_is_not_supported() {
    let world = World::new();

    let output = world
        .command("scripts/session-start.sh")
        .env("NV_TEST_UNAME", "Linux riscv64")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(context_of(&output).contains("riscv64"));
}

#[test]
fn quotes_and_backslashes_in_the_reason_do_not_break_the_json() {
    let world = World::new();
    // The reason names the folder; make the folder name hard to put into JSON.
    let odd = world.path("home/a\"b\\c");

    let output = world
        .command("scripts/session-start.sh")
        .env("NV_BIN_DIR", &odd)
        .env("NV_NO_DOWNLOAD", "1")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(context_of(&output).contains("a\"b\\c"));
}

#[test]
fn hooks_json_runs_the_script_for_every_kind_of_session_start() {
    let text = fs::read_to_string(root().join("plugin/hooks/hooks.json")).unwrap();
    let hooks: Value = serde_json::from_str(&text).unwrap();

    let group = &hooks["hooks"]["SessionStart"][0];
    let matcher = group["matcher"].as_str().unwrap();
    // After /clear and after a compaction the context is gone: say it again.
    for kind in ["startup", "resume", "clear", "compact"] {
        assert!(
            matcher.split('|').any(|part| part == kind),
            "{kind} is not in {matcher}"
        );
    }
    let hook = &group["hooks"][0];
    assert_eq!(hook["type"], "command");
    let command = hook["command"].as_str().unwrap();
    assert!(command.contains("${CLAUDE_PLUGIN_ROOT}"), "{command}");
    assert!(command.contains("scripts/session-start.sh"), "{command}");
    // The first run downloads about 160 MB.
    assert!(hook["timeout"].as_u64().unwrap() >= 600);
}

#[test]
fn trigger_text_names_both_skills_and_the_old_claude_md_block_is_gone() {
    let trigger = trigger_text();

    for word in ["`nv`", "nv:capture", "nv:recall", "decision", "promise"] {
        assert!(
            trigger.contains(word),
            "the trigger text does not mention {word}"
        );
    }
    assert!(!root().join("claude/CLAUDE.snippet.md").exists());
}

#[test]
fn the_scripts_of_the_plugin_are_executable() {
    for script in ["bin/nv", "scripts/ensure-nv.sh", "scripts/session-start.sh"] {
        let path = root().join("plugin").join(script);
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o111,
            0o111,
            "{} is not executable",
            Path::new(script).display()
        );
    }
}
