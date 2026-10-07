//! `install.sh` (the developer install) puts the binary, the model, the plugin, the
//! permissions in place, removes the old CLAUDE.md lines, and can be run again without
//! doubling anything. The `claude` CLI is a stub that writes its arguments to a log.

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
            .env("NV_LINK_DIR", self.dir.path().join("link"))
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
    fs::create_dir_all(dir.join("plugin")).unwrap();
    fs::copy(repo().join("plugin/VERSION"), dir.join("plugin/VERSION")).unwrap();
    fs::create_dir_all(dir.join("claude")).unwrap();
    fs::copy(
        repo().join("claude/settings.snippet.json"),
        dir.join("claude/settings.snippet.json"),
    )
    .unwrap();
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
    let claude_md = "# Mine\n\n# nv:start\nold text\n# nv:end\n";
    fs::write(target.claude().join("CLAUDE.md"), claude_md).unwrap();

    let output = target.install(&script, &["--dry-run"]);

    let said = String::from_utf8(output.stdout).unwrap();
    for step in [
        "cargo build --release",
        concat!("bin/nv-", env!("CARGO_PKG_VERSION")),
        "link/nv",
        "old standalone skill, removed",
        "skills/nv-capture",
        "claude plugin marketplace add",
        "claude plugin install nv@nv-marketplace",
        "Bash(nv:*), Skill(nv:capture), Skill(nv:recall)",
        "CLAUDE.md.before-nv",
        "old nv lines (# nv:start ... # nv:end)",
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
    assert!(!target.dir.path().join("link").exists());
    assert_eq!(
        fs::read_to_string(target.claude().join("CLAUDE.md")).unwrap(),
        claude_md
    );
    assert!(!target.claude().join("CLAUDE.md.before-nv").exists());
    // The CLI may be asked what exists; it is not asked to add or install anything.
    let calls = target.claude_calls();
    assert!(!calls.contains("marketplace add"), "{calls}");
    assert!(!calls.contains("plugin install"), "{calls}");
}

#[test]
#[ignore = "builds a release binary and needs the model in models/"]
fn install_twice_removes_the_old_nv_block_once_and_keeps_one_of_each_permission() {
    let target = Target::new();
    let claude_md = target.claude().join("CLAUDE.md");
    let original =
        "# Mine\nAlways answer briefly.\n\n# nv:start\nold text\n# nv:end\n\n## After\nKeep me.\n";
    fs::write(&claude_md, original).unwrap();
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
        "# Mine\nAlways answer briefly.\n\n## After\nKeep me.\n"
    );
    assert_eq!(twice, once);
    assert_eq!(
        fs::read_to_string(target.claude().join("CLAUDE.md.before-nv")).unwrap(),
        original
    );
    // The binary is where the plugin's launcher looks for it; `nv` in a terminal is a link.
    let binary = target
        .dir
        .path()
        .join(concat!("bin/nv-", env!("CARGO_PKG_VERSION")));
    assert!(binary.is_file());
    assert_eq!(
        fs::read_link(target.dir.path().join("link/nv")).unwrap(),
        binary
    );
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
fn install_removes_the_old_standalone_skills_and_leaves_a_claude_md_without_a_block_alone() {
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

    target.install(&repo().join("install.sh"), &[]);

    for old in ["nv", "nv-recall", "nv-capture"] {
        assert!(!target.claude().join("skills").join(old).exists(), "{old}");
    }
    assert!(other_skill.join("SKILL.md").is_file());
    assert_eq!(
        fs::read_to_string(target.claude().join("CLAUDE.md")).unwrap(),
        "# Mine\n"
    );
    assert!(!target.claude().join("CLAUDE.md.before-nv").exists());
}

#[test]
#[ignore = "builds a release binary and needs the model in models/"]
fn install_creates_settings_and_removes_a_claude_md_that_only_held_the_nv_lines() {
    let target = Target::new();
    let claude_md = target.claude().join("CLAUDE.md");
    fs::write(&claude_md, "# nv:start\nold text\n# nv:end\n").unwrap();

    target.install(&repo().join("install.sh"), &[]);

    assert!(!claude_md.exists());
    assert!(target.claude().join("CLAUDE.md.before-nv").is_file());
    assert_eq!(
        fs::read_to_string(target.claude().join("settings.json")).unwrap(),
        fs::read_to_string(repo().join("claude/settings.snippet.json")).unwrap()
    );
}

#[test]
fn install_refuses_when_another_nv_is_on_the_path() {
    let source = TempDir::new().unwrap();
    let script = repo_without_build(source.path());
    let target = Target::new();
    let elsewhere = target.dir.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    fs::write(elsewhere.join("nv"), "#!/bin/sh\n").unwrap();
    fs::set_permissions(elsewhere.join("nv"), fs::Permissions::from_mode(0o755)).unwrap();

    let output = Command::new("bash")
        .arg(&script)
        .arg("--dry-run")
        .env("NV_BIN_DIR", target.dir.path().join("bin"))
        .env("NV_LINK_DIR", target.dir.path().join("link"))
        .env("NV_HOME", target.dir.path().join("nv-home"))
        .env("CLAUDE_CONFIG_DIR", target.claude())
        .env("STUB_LOG", target.dir.path().join("claude-calls.log"))
        .env(
            "PATH",
            format!(
                "{}:{}",
                elsewhere.display(),
                target.path().to_string_lossy()
            ),
        )
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(said.contains("another nv is on your PATH"), "{said}");
    assert!(said.contains("elsewhere"), "{said}");
}

#[test]
#[ignore = "builds a release binary and needs the model in models/"]
fn install_replaces_an_nv_that_an_earlier_install_left_in_the_link_folder() {
    let target = Target::new();
    let link = target.dir.path().join("link");
    fs::create_dir(&link).unwrap();
    fs::write(link.join("nv"), "an old copy of the binary").unwrap();

    let output = Command::new("bash")
        .arg(repo().join("install.sh"))
        .env("NV_BIN_DIR", target.dir.path().join("bin"))
        .env("NV_LINK_DIR", &link)
        .env("NV_HOME", target.dir.path().join("nv-home"))
        .env("CLAUDE_CONFIG_DIR", target.claude())
        .env("STUB_LOG", target.dir.path().join("claude-calls.log"))
        // The link folder is on the PATH, as ~/.local/bin is for a real user.
        .env(
            "PATH",
            format!("{}:{}", link.display(), target.path().to_string_lossy()),
        )
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_link(link.join("nv")).unwrap(),
        target
            .dir
            .path()
            .join(concat!("bin/nv-", env!("CARGO_PKG_VERSION")))
    );
}
