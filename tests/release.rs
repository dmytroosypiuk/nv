//! The GitHub workflows must agree with what `plugin/scripts/ensure-nv.sh` downloads:
//! same file names, same targets, same SHA256SUMS. GitHub does the YAML check; these tests
//! catch the mistakes that YAML cannot: a renamed asset, a wrong runner, a missing guard.

mod common;

use std::fs;
use std::path::Path;

use common::root;

fn file(path: &str) -> String {
    fs::read_to_string(root().join(path)).unwrap_or_else(|_| panic!("cannot read {path}"))
}

#[test]
fn the_release_builds_what_the_fetcher_downloads() {
    let release = file(".github/workflows/release.yml");
    let fetcher = file("plugin/scripts/ensure-nv.sh");

    // The targets the fetcher knows are the targets the release builds, and no others.
    for target in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        assert!(
            fetcher.contains(target),
            "the fetcher does not know {target}"
        );
        assert!(
            release.contains(target),
            "the release does not build {target}"
        );
    }
    assert_eq!(
        release.matches("target: ").count(),
        2,
        "one build for each target"
    );
    // The names of the files.
    assert!(release.contains("nv-${VERSION}-${{ matrix.target }}.tar.gz"));
    assert!(fetcher.contains("nv-$VERSION-$TARGET.tar.gz"));
    assert!(release.contains("model-bge-small-en-v1.5.tar.gz"));
    assert!(fetcher.contains("model-$MODEL_NAME.tar.gz"));
    assert!(release.contains("SHA256SUMS"));
    assert!(fetcher.contains("SHA256SUMS"));
    // The archives hold what the fetcher unpacks: `nv`, and `bge-small-en-v1.5/`.
    assert!(release.contains("-C target/${{ matrix.target }}/release nv"));
    assert!(release.contains("-C models bge-small-en-v1.5"));
}

#[test]
fn linux_is_built_on_the_oldest_image_that_can_link_onnx_runtime() {
    let release = file(".github/workflows/release.yml");

    // The prebuilt ONNX Runtime needs glibc 2.38 symbols and a GCC 13 libstdc++: on
    // ubuntu-22.04 the link fails (found by the first CI run, 2026-10-07).
    assert!(release.contains("ubuntu-24.04"));
    assert!(
        !release.contains("ubuntu-22.04"),
        "ubuntu-22.04 cannot link it"
    );
    assert!(
        !release.contains("ubuntu-latest"),
        "a new image raises the glibc floor"
    );
    // The glibc the binary needs is printed in the log of every release.
    assert!(release.contains("GLIBC_"));
    // macos-14 is Apple silicon; macos-13 and macos-latest may be Intel.
    assert!(release.contains("macos-14"));
    assert!(!release.contains("macos-13") && !release.contains("macos-latest"));
}

#[test]
fn a_tag_must_equal_the_cargo_version_and_the_plugin_version() {
    let release = file(".github/workflows/release.yml");

    for needle in [
        "Cargo.toml",
        "plugin/VERSION",
        "plugin/.claude-plugin/plugin.json",
        "GITHUB_REF_NAME",
    ] {
        assert!(
            release.contains(needle),
            "the version guard does not look at {needle}"
        );
    }
}

#[test]
fn only_a_tag_publishes_and_a_manual_run_is_a_dry_run() {
    let release = file(".github/workflows/release.yml");

    assert!(release.contains("workflow_dispatch"));
    assert!(release.contains("startsWith(github.ref, 'refs/tags/v')"));
    assert!(release.contains("contents: write"));
    assert!(release.contains("gh release create"));
    // A tag like v0.2.0-rc1 is a pre-release.
    assert!(release.contains("--prerelease"));
    // Builds are reproducible from the lock file.
    assert_eq!(release.matches("--locked").count(), 1);
}

#[test]
fn the_model_of_a_release_is_checked_against_the_pinned_checksums() {
    let release = file(".github/workflows/release.yml");
    let pinned = file("spikes/embedding/model.sha256");

    assert!(release.contains("sha256sum -c"));
    assert!(release.contains("spikes/embedding/model.sha256"));
    let names: Vec<&str> = pinned
        .lines()
        .map(|line| line.split_whitespace().nth(1).unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "config.json",
            "model.onnx",
            "special_tokens_map.json",
            "tokenizer.json",
            "tokenizer_config.json"
        ]
    );
}

/// Only where the model is here (a dev machine, not CI): the pin is not stale.
#[test]
fn the_pinned_checksums_match_the_model_on_this_machine() {
    let folder = root().join("models/bge-small-en-v1.5");
    if !Path::new(&folder).join("model.onnx").exists() {
        return;
    }
    let status = std::process::Command::new("sha256sum")
        .arg("-c")
        .arg(root().join("spikes/embedding/model.sha256"))
        .current_dir(&folder)
        .output();
    let Ok(output) = status else { return }; // no sha256sum (macOS): nothing to say

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn ci_checks_format_lints_and_tests_on_both_platforms() {
    let ci = file(".github/workflows/ci.yml");

    for needle in [
        "cargo fmt --check",
        "cargo clippy --all-targets --locked -- -D warnings",
        "cargo test --locked",
        "ubuntu-24.04",
        "macos-14",
        "pull_request",
    ] {
        assert!(ci.contains(needle), "ci.yml does not contain {needle}");
    }
}

#[test]
fn the_release_files_carry_the_licence_notices() {
    let release = file(".github/workflows/release.yml");

    // The binary archive holds the licence and the notices next to `nv` (ONNX Runtime is
    // inside the binary); the model archive holds the notice of the model.
    assert!(release.contains("LICENSE THIRD_PARTY_NOTICES.md"));
    assert!(release.contains("models/bge-small-en-v1.5/NOTICE"));
}
