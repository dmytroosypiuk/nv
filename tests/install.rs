//! `install.sh` (the developer install) puts the binary, the model, the plugin, the
//! permissions and the CLAUDE.md lines in place, and can be run again without doubling
//! anything. The `claude` CLI is a stub that writes its arguments to a log.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

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
        // A `claude` that only logs its arguments, so the real plugin list is not touched.
        let stub = dir.path().join("stub");
        fs::create_dir(&stub).unwrap();
        let script = stub.join("claude");
        fs::write(&script, "#!/bin/sh\necho \"$@\" >> \"$STUB_LOG\"\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }

    fn claude(&self) -> PathBuf {
        self.dir.path().join("claude")
    }

    /// What the stub `claude` was asked to do, one command per line.
    fn claude_calls(&self) -> String {
        fs::read_to_string(self.dir.path().join("claude-calls.log")).unwrap_or_default()
    }

    fn install(&self, script: &Path, args: &[&str]) -> Output {
        let output = Command::new("bash")
            .arg(script)
            .args(args)
            .env("NV_BIN_DIR", self.dir.path().join("bin"))
            .env("NV_HOME", self.dir.path().join("nv-home"))
            .env("CLAUDE_CONFIG_DIR", self.claude())
            .env("STUB_LOG", self.dir.path().join("claude-calls.log"))
            .env("PATH", self.path())
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

    fn path(&self) -> std::ffi::OsString {
        let mut folders = vec![self.dir.path().join("stub")];
        folders.extend(std::env::split_paths(&path_without_nv()));
        std::env::join_paths(folders).unwrap()
    }
}

/// The PATH without the folders that hold an installed `nv`: the script refuses to
/// install next to another nv, and the test must not depend on this machine.
fn path_without_nv() -> std::ffi::OsString {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let kept = std::env::split_paths(&path).filter(|dir| !dir.join("nv").exists());
    std::env::join_paths(kept).unwrap()
}

/// A copy of what the script reads from the repo, with an empty file for the model, so a
/// dry run needs neither the real model nor a build.
fn repo_without_build(dir: &Path) -> PathBuf {
    let model = dir.join("models/bge-small-en-v1.5");
    fs::create_dir_all(&model).unwrap();
    fs::write(model.join("model.onnx"), "not a model").unwrap();
    fs::copy(repo().join("install.sh"), dir.join("install.sh")).unwrap();
    fs::create_dir_all(dir.join("claude")).unwrap();
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
    for old in ["nv", "nv-recall", "nv-capture"] {
        fs::create_dir_all(target.claude().join("skills").join(old)).unwrap();
    }
    fs::write(target.claude().join("CLAUDE.md"), "# Mine\n").unwrap();

    let output = target.install(&script, &["--dry-run"]);

    let said = String::from_utf8(output.stdout).unwrap();
    for step in [
        "cargo build --release",
        "old standalone skill, removed",
        "skills/nv-capture",
        "claude plugin marketplace add",
        "claude plugin install nv@nv-marketplace",
        "Bash(nv:*), Skill(nv:capture), Skill(nv:recall)",
        "CLAUDE.md.before-nv",
        "nv:start",
        "Dry run: nothing was changed.",
    ] {
        assert!(
            said.contains(step),
            "the dry run does not name {step}:\n{said}"
        );
    }
    for old in ["nv", "nv-recall", "nv-capture"] {
        assert!(target.claude().join("skills").join(old).is_dir(), "{old}");
    }
    assert!(!target.claude().join("settings.json").exists());
    assert!(!target.dir.path().join("bin").exists());
    assert_eq!(
        fs::read_to_string(target.claude().join("CLAUDE.md")).unwrap(),
        "# Mine\n"
    );
    // The CLI may be asked what exists; it is not asked to add or install anything.
    let calls = target.claude_calls();
    assert!(!calls.contains("marketplace add"), "{calls}");
    assert!(!calls.contains("plugin install"), "{calls}");
}

#[test]
#[ignore = "builds a release binary and needs the model in models/"]
fn install_twice_keeps_one_nv_block_and_one_of_each_permission() {
    let target = Target::new();
    let claude_md = target.claude().join("CLAUDE.md");
    let snippet = fs::read_to_string(repo().join("claude/CLAUDE.snippet.md")).unwrap();
    // An older nv block between the user's own lines, and settings of the user's own.
    fs::write(
        &claude_md,
        "# Mine\nAlways answer briefly.\n\n# nv:start\nold text\n# nv:end\n\n## After\nKeep me.\n",
    )
    .unwrap();
    fs::write(
        target.claude().join("settings.json"),
        r#"{"model": "sonnet", "permissions": {"allow": ["Bash(ls:*)", "Bash(nv:*)"]}}"#,
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
    assert!(target.dir.path().join("bin/nv").is_file());
    let settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(target.claude().join("settings.json")).unwrap())
            .unwrap();
    assert_eq!(settings["model"], "sonnet");
    assert_eq!(
        settings["permissions"]["allow"],
        serde_json::json!([
            "Bash(ls:*)",
            "Bash(nv:*)",
            "Skill(nv:capture)",
            "Skill(nv:recall)"
        ])
    );
    let calls = target.claude_calls();
    assert!(calls.contains("plugin marketplace add"), "{calls}");
    assert!(
        calls.contains("plugin install nv@nv-marketplace"),
        "{calls}"
    );
}

#[test]
#[ignore = "builds a release binary and needs the model in models/"]
fn install_removes_the_old_standalone_skills_and_adds_the_block_to_a_file_without_one() {
    let target = Target::new();
    for old in ["nv", "nv-recall", "nv-capture"] {
        let folder = target.claude().join("skills").join(old);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("SKILL.md"), "old").unwrap();
    }
    let other_skill = target.claude().join("skills/other");
    fs::create_dir_all(&other_skill).unwrap();
    fs::write(other_skill.join("SKILL.md"), "not ours").unwrap();
    fs::write(target.claude().join("CLAUDE.md"), "# Mine\n").unwrap();
    let snippet = fs::read_to_string(repo().join("claude/CLAUDE.snippet.md")).unwrap();

    target.install(&repo().join("install.sh"), &[]);

    for old in ["nv", "nv-recall", "nv-capture"] {
        assert!(!target.claude().join("skills").join(old).exists(), "{old}");
    }
    assert!(other_skill.join("SKILL.md").is_file());
    assert_eq!(
        fs::read_to_string(target.claude().join("CLAUDE.md")).unwrap(),
        format!("# Mine\n\n{snippet}")
    );
}

#[test]
#[ignore = "builds a release binary and needs the model in models/"]
fn install_creates_claude_md_and_settings_when_there_are_none() {
    let target = Target::new();
    let snippet = fs::read_to_string(repo().join("claude/CLAUDE.snippet.md")).unwrap();

    target.install(&repo().join("install.sh"), &[]);

    assert_eq!(
        fs::read_to_string(target.claude().join("CLAUDE.md")).unwrap(),
        snippet
    );
    assert!(!target.claude().join("CLAUDE.md.before-nv").exists());
    assert_eq!(
        fs::read_to_string(target.claude().join("settings.json")).unwrap(),
        fs::read_to_string(repo().join("claude/settings.snippet.json")).unwrap()
    );
}
