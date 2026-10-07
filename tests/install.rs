//! `install.sh` puts the binary, the model, the two skills, the permission and the
//! CLAUDE.md lines in place, and can be run again without doubling anything.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const SKILL_FILES: [&str; 6] = [
    "nv-recall/SKILL.md",
    "nv-recall/cli-read.md",
    "nv-capture/SKILL.md",
    "nv-capture/saving-rules.md",
    "nv-capture/template.md",
    "nv-capture/cli-write.md",
];

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Where one install goes: nothing outside this folder is touched.
struct Target {
    dir: TempDir,
}

impl Target {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("claude")).unwrap();
        Self { dir }
    }

    fn claude(&self) -> PathBuf {
        self.dir.path().join("claude")
    }

    fn install(&self, script: &Path, args: &[&str]) -> Output {
        let output = Command::new("bash")
            .arg(script)
            .args(args)
            .env("NV_BIN_DIR", self.dir.path().join("bin"))
            .env("NV_HOME", self.dir.path().join("nv-home"))
            .env("CLAUDE_CONFIG_DIR", self.claude())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}

/// A copy of what the script reads from the repo, with an empty file for the model, so a
/// dry run needs neither the real model nor a build.
fn repo_without_build(dir: &Path) -> PathBuf {
    let model = dir.join("models/bge-small-en-v1.5");
    fs::create_dir_all(&model).unwrap();
    fs::write(model.join("model.onnx"), "not a model").unwrap();
    fs::copy(repo().join("install.sh"), dir.join("install.sh")).unwrap();
    for file in SKILL_FILES {
        let to = dir.join("claude/skills").join(file);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(repo().join("claude/skills").join(file), to).unwrap();
    }
    for file in ["CLAUDE.snippet.md", "settings.snippet.json"] {
        fs::copy(
            repo().join("claude").join(file),
            dir.join("claude").join(file),
        )
        .unwrap();
    }
    dir.join("install.sh")
}

#[test]
fn dry_run_changes_nothing_and_names_every_step() {
    let source = TempDir::new().unwrap();
    let script = repo_without_build(source.path());
    let target = Target::new();
    fs::create_dir_all(target.claude().join("skills/nv")).unwrap();
    fs::write(target.claude().join("CLAUDE.md"), "# Mine\n").unwrap();

    let output = target.install(&script, &["--dry-run"]);

    let said = String::from_utf8(output.stdout).unwrap();
    for step in [
        "cargo build --release",
        "skills/nv-recall",
        "skills/nv-capture",
        "skills/nv\n",
        "Bash(nv:*)",
        "CLAUDE.md.before-nv",
        "nv:start",
        "Dry run: nothing was changed.",
    ] {
        assert!(
            said.contains(step),
            "the dry run does not name {step}:\n{said}"
        );
    }
    assert!(target.claude().join("skills/nv").is_dir());
    assert!(!target.claude().join("skills/nv-recall").exists());
    assert!(!target.claude().join("settings.json").exists());
    assert!(!target.dir.path().join("bin").exists());
    assert_eq!(
        fs::read_to_string(target.claude().join("CLAUDE.md")).unwrap(),
        "# Mine\n"
    );
}

#[test]
#[ignore = "builds a release binary and needs the model in models/"]
fn install_twice_keeps_one_nv_block_and_other_lines() {
    let target = Target::new();
    let claude_md = target.claude().join("CLAUDE.md");
    let snippet = fs::read_to_string(repo().join("claude/CLAUDE.snippet.md")).unwrap();
    // An older nv block between the user's own lines.
    fs::write(
        &claude_md,
        "# Mine\nAlways answer briefly.\n\n# nv:start\nold text\n# nv:end\n\n## After\nKeep me.\n",
    )
    .unwrap();

    target.install(&repo().join("install.sh"), &[]);
    let once = fs::read_to_string(&claude_md).unwrap();
    target.install(&repo().join("install.sh"), &[]);
    let twice = fs::read_to_string(&claude_md).unwrap();

    assert_eq!(
        once,
        format!("# Mine\nAlways answer briefly.\n\n{snippet}\n## After\nKeep me.\n")
    );
    assert_eq!(twice, once);
    assert_eq!(
        fs::read_to_string(target.claude().join("CLAUDE.md.before-nv")).unwrap(),
        "# Mine\nAlways answer briefly.\n\n# nv:start\nold text\n# nv:end\n\n## After\nKeep me.\n"
    );
    for file in SKILL_FILES {
        assert!(
            target.claude().join("skills").join(file).is_file(),
            "{file}"
        );
    }
    assert!(target.dir.path().join("bin/nv").is_file());
    let settings = fs::read_to_string(target.claude().join("settings.json")).unwrap();
    assert_eq!(settings.matches("Bash(nv:*)").count(), 1);
}

#[test]
#[ignore = "builds a release binary and needs the model in models/"]
fn install_removes_the_old_nv_skill_and_adds_the_block_to_a_file_without_one() {
    let target = Target::new();
    let old_skill = target.claude().join("skills/nv");
    fs::create_dir_all(&old_skill).unwrap();
    fs::write(old_skill.join("SKILL.md"), "old").unwrap();
    let other_skill = target.claude().join("skills/other");
    fs::create_dir_all(&other_skill).unwrap();
    fs::write(other_skill.join("SKILL.md"), "not ours").unwrap();
    fs::write(target.claude().join("CLAUDE.md"), "# Mine\n").unwrap();
    let snippet = fs::read_to_string(repo().join("claude/CLAUDE.snippet.md")).unwrap();

    target.install(&repo().join("install.sh"), &[]);

    assert!(!old_skill.exists());
    assert!(other_skill.join("SKILL.md").is_file());
    assert_eq!(
        fs::read_to_string(target.claude().join("CLAUDE.md")).unwrap(),
        format!("# Mine\n\n{snippet}")
    );
}

#[test]
#[ignore = "builds a release binary and needs the model in models/"]
fn install_creates_claude_md_when_there_is_none() {
    let target = Target::new();
    let snippet = fs::read_to_string(repo().join("claude/CLAUDE.snippet.md")).unwrap();

    target.install(&repo().join("install.sh"), &[]);

    assert_eq!(
        fs::read_to_string(target.claude().join("CLAUDE.md")).unwrap(),
        snippet
    );
    assert!(!target.claude().join("CLAUDE.md.before-nv").exists());
}
