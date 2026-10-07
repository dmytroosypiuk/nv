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

/// The two skills and the commands each one must show, in `SKILL.md` or a file next to it.
const SKILLS: [(&str, &[&str]); 2] = [
    (
        "recall",
        &[
            "nv date",
            "nv search ",
            "nv note show ",
            "nv today",
            "nv people search ",
            "nv people list",
            "nv history",
        ],
    ),
    (
        "capture",
        &[
            "nv date",
            "nv search ",
            "nv add ",
            "nv note edit ",
            "nv note replace ",
            "nv note delete ",
            "nv note link ",
            "nv commitment done ",
            "nv commitment drop ",
            "nv commitment postpone ",
            "nv people search ",
            "nv people add ",
            "nv people alias ",
            "nv people merge ",
            "nv history undo",
        ],
    ),
];

fn skill_dir(skill: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("plugin/skills")
        .join(skill)
}

/// Every Markdown file of a skill, `SKILL.md` and the files it links to, as one text.
fn whole_skill(skill: &str) -> String {
    let mut files: Vec<_> = fs::read_dir(skill_dir(skill))
        .unwrap_or_else(|_| panic!("the skill {skill} has no folder"))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    files.sort();
    let texts: Vec<String> = files
        .iter()
        .map(|path| fs::read_to_string(path).unwrap())
        .collect();
    texts.join("\n")
}

fn skill_md(skill: &str) -> String {
    repo_file(&format!("plugin/skills/{skill}/SKILL.md"))
}

#[test]
fn each_skill_has_name_description_and_allowed_tools() {
    for (skill, _) in SKILLS {
        let text = skill_md(skill);
        let frontmatter = text
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .expect("SKILL.md starts with a frontmatter block")
            .0;

        let lines: Vec<&str> = frontmatter.lines().collect();
        assert!(
            lines.contains(&format!("name: {skill}").as_str()),
            "{frontmatter}"
        );
        assert!(
            lines.contains(&"allowed-tools: Bash(nv *)"),
            "{frontmatter}"
        );
        let description = frontmatter
            .lines()
            .find_map(|line| line.strip_prefix("description: "))
            .expect("a one-line description");
        assert!(
            description.len() > 80 && description.contains("Use when"),
            "the description of {skill} says when to use it"
        );
    }
}

#[test]
fn skills_use_only_commands_and_flags_that_exist() {
    for (skill, needed_commands) in SKILLS {
        let commands = nv_commands(&whole_skill(skill));
        assert!(
            commands.len() >= 10,
            "{skill}: found only {} commands",
            commands.len()
        );

        for command in &commands {
            let help = help_of(command);
            for word in command.split_whitespace() {
                if let Some(flag) = word.strip_prefix("--") {
                    let flag = flag.split('=').next().unwrap();
                    assert!(
                        help.contains(&format!("--{flag} "))
                            || help.contains(&format!("--{flag}\n")),
                        "{skill}: `--{flag}` is not a flag of: {command}"
                    );
                }
            }
        }
        // The commands Claude needs every day are all shown.
        for needed in needed_commands {
            let name = needed.trim_end();
            assert!(
                commands
                    .iter()
                    .any(|command| command == name || command.starts_with(&format!("{name} "))),
                "{skill} does not show `{name}`"
            );
        }
    }
}

#[test]
fn skill_files_linked_from_skill_md_exist() {
    for (skill, linked) in [
        ("recall", vec!["cli-read.md"]),
        (
            "capture",
            vec!["saving-rules.md", "template.md", "cli-write.md"],
        ),
    ] {
        let text = skill_md(skill);
        for file in linked {
            assert!(
                text.contains(&format!("]({file})")),
                "{skill}/SKILL.md does not link to {file}"
            );
            assert!(skill_dir(skill).join(file).is_file(), "{skill}/{file}");
        }
        // Each skill stands alone: no link into the other one.
        assert!(!text.contains("](../"), "{skill} links across skills");
        assert!(
            text.lines().count() <= 125,
            "{skill}/SKILL.md is short; long parts go to the linked files"
        );
    }
}

#[test]
fn skills_live_in_the_plugin_only() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // Before the plugin the skills were copied from here by install.sh.
    assert!(!root.join("claude/skills").exists());
    assert!(!skill_dir("nv").exists());
}

#[test]
fn skills_tell_the_rules_that_nv_cannot_check() {
    let recall = skill_md("recall").to_lowercase();
    let capture = skill_md("capture").to_lowercase();

    // The small shared rules are written in both, not linked.
    for (skill, text) in [("recall", &recall), ("capture", &capture)] {
        for rule in ["english", "real date", "by id"] {
            assert!(text.contains(rule), "{skill} does not mention: {rule}");
        }
    }
    for rule in [
        "outdated →",
        "--limit 50",
        "nv ranks, it does not judge",
        "run `nv date`",
        "do not `cd`",
    ] {
        assert!(recall.contains(rule), "recall does not mention: {rule}");
    }
    for rule in [
        "<<'eof'",
        "safety net",
        "never about whether to save",
        "planned for fri 2026-10-09",
        "flags you give override the copied fields",
    ] {
        assert!(capture.contains(rule), "capture does not mention: {rule}");
    }
}

/// A small model reads only `SKILL.md`, not the linked files: what a correct `nv add`
/// needs must be there. Found in the hand test with Haiku (2026-10-07).
#[test]
fn capture_skill_md_alone_is_enough_for_a_correct_note() {
    let capture = skill_md("capture");

    for word in [
        "`decision`",
        "`commitment`",
        "`how-to`",
        "`fact`",
        "`idea`",
        "`work`",
        "`learning`",
        "`personal`",
        "--expires-on",
        "--source-kind",
        "--owner",
        "--planned",
    ] {
        assert!(capture.contains(word), "SKILL.md does not explain {word}");
    }
    let lower = capture.to_lowercase();
    for rule in [
        "write only what the user said",
        "never copy facts from an example",
        "copy the weekday from the answer of nv",
        "one note for each item",
        "a promise by another person is its own commitment",
        "save it without `--planned`",
        "correct the date in the body too",
        "run `nv date`",
        "do not invent a source",
        "not `replace`",
        "never wait with the save",
        "save first, ask after",
        "do not `cd`",
    ] {
        assert!(lower.contains(rule), "SKILL.md does not say: {rule}");
    }
    // Haiku gave a person a role the user never said, three times: `SKILL.md` does not
    // show `--role` at all. It is in cli-write.md for when the user does say a role.
    assert!(!capture.contains("--role"), "SKILL.md shows --role");
}

#[test]
fn capture_carries_the_temporary_company_data_rule() {
    let capture = skill_md("capture");
    let one_line = capture.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(one_line.contains(
        "Until Dmytro confirms his company's rules for AI tools, keep work notes free of \
         confidential company data: no customer data, no internal hostnames, URLs or IPs, \
         no code copied from company repos, no financial figures. Decisions, promises and \
         how-tos in general words are fine. Dmytro will remove this rule."
    ));
}

#[test]
fn capture_examples_use_add_and_remove_flags_and_planned() {
    let capture = nv_commands(&whole_skill("capture")).join("\n");

    for flag in [
        "--add-repo",
        "--add-person",
        "--remove-ticket",
        "--planned ",
    ] {
        assert!(capture.contains(flag), "no example uses {flag}");
    }
    for (skill, _) in SKILLS {
        assert!(!whole_skill(skill).contains("--planned-for"), "{skill}");
    }
}

/// The skill for developers of nv, in `.claude/skills` (read by Claude Code in this
/// project). It is not installed: `plugin/skills` holds the skills of the product.
#[test]
fn agentic_testing_skill_is_a_project_skill_with_its_driver() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".claude/skills/agentic-testing");
    let text = repo_file(".claude/skills/agentic-testing/SKILL.md");

    let frontmatter = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .expect("SKILL.md starts with a frontmatter block")
        .0;
    assert!(
        frontmatter
            .lines()
            .any(|line| line == "name: agentic-testing")
    );
    let description = frontmatter
        .lines()
        .find_map(|line| line.strip_prefix("description: "))
        .expect("a one-line description");
    assert!(description.len() > 80 && description.contains("Use when"));

    // What the hand tests of 2026-10-07 taught: these are the rules that cost time.
    let lower = text.to_lowercase();
    for rule in [
        "nv_home",
        "claude_code_force_session_persistence",
        "subagent",
        "claude -p",
        "never approve",
        "names only",
        "docs/skill-test.md",
    ] {
        assert!(lower.contains(rule), "the skill does not mention: {rule}");
    }
    assert!(text.contains("](scripts/pty_run.py)"));
    assert!(dir.join("scripts/pty_run.py").is_file());
    assert!(
        !Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("plugin/skills/agentic-testing")
            .exists()
    );
    assert!(
        text.lines().count() <= 120,
        "SKILL.md is short; details go to the linked files"
    );
}

#[test]
fn settings_snippet_allows_nv_and_has_no_hook() {
    let snippet: Value = serde_json::from_str(&repo_file("claude/settings.snippet.json")).unwrap();

    assert_eq!(
        snippet["permissions"]["allow"],
        serde_json::json!(["Bash(nv:*)", "Skill(nv:capture)", "Skill(nv:recall)"])
    );
    // Decided in step 7: no SessionStart hook.
    assert!(snippet.get("hooks").is_none());
}
