//! The Claude Code plugin and the marketplace that lists it: names, versions and layout.
//! `claude plugin validate` has the last word; it is the `#[ignore]` test at the end.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn json(path: &str) -> Value {
    let path = root().join(path);
    let text =
        fs::read_to_string(&path).unwrap_or_else(|_| panic!("cannot read {}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn marketplace_lists_the_plugin_and_its_names_are_not_reserved() {
    let marketplace = json(".claude-plugin/marketplace.json");
    let plugin = json("plugin/.claude-plugin/plugin.json");

    assert_eq!(marketplace["name"], "nv-marketplace");
    assert!(
        marketplace["owner"]["name"]
            .as_str()
            .is_some_and(|name| !name.is_empty())
    );
    let entries = marketplace["plugins"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["name"], "nv");
    assert_eq!(entries[0]["source"], "./plugin");
    // Users type the entry name; the skills are prefixed with the manifest name.
    assert_eq!(entries[0]["name"], plugin["name"]);
    for name in [&marketplace["name"], &plugin["name"]] {
        let name = name.as_str().unwrap().to_lowercase();
        for reserved in ["claude", "anthropic", "official"] {
            assert!(!name.contains(reserved), "{name} passes as Anthropic's own");
        }
    }
}

#[test]
fn plugin_version_is_the_cargo_version_everywhere() {
    let plugin = json("plugin/.claude-plugin/plugin.json");
    let version_file = fs::read_to_string(root().join("plugin/VERSION")).unwrap();

    assert_eq!(plugin["version"], env!("CARGO_PKG_VERSION"));
    // The shell scripts read this file: plain text, one line.
    assert_eq!(version_file, format!("{}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn plugin_manifest_has_the_fields_a_user_sees() {
    let plugin = json("plugin/.claude-plugin/plugin.json");

    assert_eq!(plugin["name"], "nv");
    for field in ["description", "repository", "author", "license"] {
        assert!(!plugin[field].is_null(), "plugin.json has no {field}");
    }
    assert_eq!(plugin["repository"], "https://github.com/dmytroosypiuk/nv");
}

#[test]
fn plugin_has_two_skills_named_like_their_folders() {
    for skill in ["capture", "recall"] {
        let text = fs::read_to_string(root().join(format!("plugin/skills/{skill}/SKILL.md")))
            .unwrap_or_else(|_| panic!("the plugin has no skill {skill}"));

        assert!(
            text.lines().any(|line| line == format!("name: {skill}")),
            "plugin/skills/{skill}/SKILL.md must say `name: {skill}`"
        );
    }
}

/// Needs the `claude` CLI: `cargo test -- --ignored`.
#[test]
#[ignore = "needs the claude CLI"]
fn claude_plugin_validate_passes_for_the_marketplace_and_the_plugin() {
    for path in [".", "plugin"] {
        let output = Command::new("claude")
            .args(["plugin", "validate", path])
            .current_dir(root())
            .output()
            .expect("the claude CLI is on the PATH");
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(output.status.success(), "validate {path}:\n{said}");
        assert!(!said.contains("warning"), "validate {path} warns:\n{said}");
    }
}

#[test]
fn the_project_is_mit_licensed_and_says_so_everywhere() {
    let license = fs::read_to_string(root().join("LICENSE")).unwrap();
    let cargo = fs::read_to_string(root().join("Cargo.toml")).unwrap();
    let readme = fs::read_to_string(root().join("README.md")).unwrap();

    assert!(license.starts_with("MIT License"));
    assert!(license.contains("Copyright (c) 2026 dmytroosypiuk"));
    assert!(license.contains("Permission is hereby granted, free of charge"));
    assert!(cargo.lines().any(|line| line == r#"license = "MIT""#));
    assert_eq!(json("plugin/.claude-plugin/plugin.json")["license"], "MIT");
    assert!(readme.contains("## License"));
    assert!(readme.contains("THIRD_PARTY_NOTICES.md"));
}

#[test]
fn third_party_notices_name_the_model_and_onnx_runtime() {
    let notices = fs::read_to_string(root().join("THIRD_PARTY_NOTICES.md")).unwrap();

    for needle in [
        "bge-small-en-v1.5",
        "BAAI",
        "ONNX Runtime",
        "MIT",
        "Cargo.lock",
    ] {
        assert!(
            notices.contains(needle),
            "THIRD_PARTY_NOTICES.md does not mention {needle}"
        );
    }
}
