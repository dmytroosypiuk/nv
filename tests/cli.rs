use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::TempDir;

/// `nv` with a temporary `NV_HOME`, so tests never touch the real one.
fn nv(nv_home: &TempDir) -> Command {
    let mut command = cargo_bin_cmd!("nv");
    command.env("NV_HOME", nv_home.path());
    command
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
