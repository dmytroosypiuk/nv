//! Keeps the Claude Code skill in step with the CLI: a renamed command or flag must
//! break the build, not the skill.

use std::fs;
use std::path::Path;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;

fn repo_file(path: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("cannot read {}", path.display()))
}

/// The `nv …` commands in the fenced code blocks of the skill, continuation lines joined.
fn nv_commands(skill: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut in_code_block = false;
    let mut continued: Option<String> = None;
    for line in skill.lines() {
        let line = line.trim();
        if line.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if !in_code_block {
            continue;
        }
        let mut command = match continued.take() {
            Some(start) => format!("{start} {line}"),
            None if line.starts_with("nv ") => line.to_string(),
            None => continue,
        };
        if command.ends_with('\\') {
            command.pop();
            continued = Some(command.trim_end().to_string());
        } else {
            commands.push(command);
        }
    }
    commands
}

/// The help text of the subcommand a command line uses, like `nv note edit --help`.
fn help_of(command: &str) -> String {
    let words: Vec<&str> = command.split_whitespace().skip(1).collect();
    let is_word = |word: &&str| word.chars().all(|c| c.is_ascii_lowercase() || c == '-');
    let mut candidates = vec![&words[..1]];
    if words.len() > 1 && is_word(&words[1]) && !words[1].starts_with('-') {
        candidates.insert(0, &words[..2]);
    }
    for path in candidates {
        let output = cargo_bin_cmd!("nv")
            .args(path)
            .arg("--help")
            .output()
            .unwrap();
        if output.status.success() {
            return String::from_utf8(output.stdout).unwrap();
        }
    }
    panic!("the skill uses a command nv does not have: {command}");
}

#[test]
fn skill_has_name_and_description() {
    let skill = repo_file("claude/skills/nv/SKILL.md");

    let frontmatter = skill
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .expect("SKILL.md starts with a frontmatter block")
        .0;

    assert!(
        frontmatter.lines().any(|line| line == "name: nv"),
        "{frontmatter}"
    );
    let description = frontmatter
        .lines()
        .find_map(|line| line.strip_prefix("description: "))
        .expect("a one-line description");
    assert!(
        description.len() > 80,
        "the description says when to use nv"
    );
}

#[test]
fn skill_uses_only_commands_and_flags_that_exist() {
    let skill = repo_file("claude/skills/nv/SKILL.md");
    let commands = nv_commands(&skill);
    assert!(
        commands.len() >= 15,
        "found only {} commands",
        commands.len()
    );

    for command in &commands {
        let help = help_of(command);
        for word in command.split_whitespace() {
            if let Some(flag) = word.strip_prefix("--") {
                let flag = flag.split('=').next().unwrap();
                assert!(
                    help.contains(&format!("--{flag}")),
                    "`--{flag}` is not a flag of: {command}"
                );
            }
        }
    }
    // The commands Claude needs every day are all shown.
    for needed in [
        "nv add ",
        "nv search ",
        "nv today",
        "nv note edit ",
        "nv note replace ",
        "nv note delete ",
        "nv note show ",
        "nv note link ",
        "nv commitment done ",
        "nv commitment drop ",
        "nv commitment postpone ",
        "nv people search ",
        "nv people add ",
        "nv people alias ",
        "nv people merge ",
        "nv history",
        "nv history undo",
    ] {
        assert!(
            commands
                .iter()
                .any(|command| command.starts_with(needed.trim_end())
                    && (command.len() == needed.trim_end().len() || command.starts_with(needed))),
            "the skill does not show `{}`",
            needed.trim_end()
        );
    }
}

#[test]
fn skill_tells_the_rules_that_nv_cannot_check() {
    let skill = repo_file("claude/skills/nv/SKILL.md").to_lowercase();

    for rule in ["english", "secret", "<<'eof'", "real date", "outdated →"] {
        assert!(skill.contains(rule), "the skill does not mention: {rule}");
    }
}

#[test]
fn settings_snippet_allows_nv_and_has_no_hook() {
    let snippet: Value = serde_json::from_str(&repo_file("claude/settings.snippet.json")).unwrap();

    assert_eq!(
        snippet["permissions"]["allow"],
        serde_json::json!(["Bash(nv:*)"])
    );
    // Decided in step 7: no SessionStart hook.
    assert!(snippet.get("hooks").is_none());
}
