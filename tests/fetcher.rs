//! The plugin's launcher (`plugin/bin/nv`) and `plugin/scripts/ensure-nv.sh`: they fetch the
//! binary and the model once, check the SHA-256, and never fetch what is already there.
//! The release is a folder of files read through `file://`; the "binary" is a shell stub.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use sha2::{Digest, Sha256};
use tempfile::TempDir;

const ASSET: &str = "nv-0.1.0-x86_64-unknown-linux-gnu.tar.gz";
const MODEL_ASSET: &str = "model-bge-small-en-v1.5.tar.gz";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sha256(path: &Path) -> String {
    let digest = Sha256::digest(fs::read(path).unwrap());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn tar(archive: &Path, folder: &Path, entries: &[&str]) {
    let status = Command::new("tar")
        .arg("-czf")
        .arg(archive)
        .arg("-C")
        .arg(folder)
        .args(entries)
        .status()
        .unwrap();
    assert!(status.success());
}

/// A machine with nothing installed, and a release to fetch from.
struct World {
    dir: TempDir,
}

impl World {
    fn new() -> Self {
        let world = Self {
            dir: TempDir::new().unwrap(),
        };
        for folder in ["release", "home", "tmp", "stub"] {
            fs::create_dir(world.path(folder)).unwrap();
        }
        world.publish_release();
        world
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// The binary is a stub that echoes its arguments and stdin and exits as told.
    fn publish_release(&self) {
        let work = self.path("work");
        fs::create_dir_all(work.join("bin")).unwrap();
        let stub = work.join("bin/nv");
        fs::write(
            &stub,
            "#!/bin/sh\necho \"stub args: $*\"\ncat\nexit \"${STUB_EXIT:-0}\"\n",
        )
        .unwrap();
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
        tar(
            &self.path("release").join(ASSET),
            &work.join("bin"),
            &["nv"],
        );

        let model = work.join("model/bge-small-en-v1.5");
        fs::create_dir_all(&model).unwrap();
        fs::write(model.join("model.onnx"), "the model").unwrap();
        fs::write(model.join("tokenizer.json"), "the tokenizer").unwrap();
        tar(
            &self.path("release").join(MODEL_ASSET),
            &work.join("model"),
            &["bge-small-en-v1.5"],
        );

        let sums: String = [ASSET, MODEL_ASSET]
            .iter()
            .map(|asset| format!("{}  {asset}\n", sha256(&self.path("release").join(asset))))
            .collect();
        fs::write(self.path("release/SHA256SUMS"), sums).unwrap();
    }

    fn unpublish_release(&self) {
        fs::remove_dir_all(self.path("release")).unwrap();
    }

    fn command(&self, script: &str) -> Command {
        let mut command = Command::new("sh");
        command
            .arg(root().join("plugin").join(script))
            .env("HOME", self.path("home"))
            .env("NV_HOME", self.path("home/nvdata"))
            .env("NV_BIN_DIR", self.path("home/bin"))
            .env(
                "NV_RELEASE_BASE",
                format!("file://{}", self.path("release").display()),
            )
            .env("NV_TEST_UNAME", "Linux x86_64")
            .env("TMPDIR", self.path("tmp"))
            .env_remove("NV_NO_DOWNLOAD");
        command
    }

    fn run(&self, script: &str, args: &[&str]) -> Output {
        self.command(script).args(args).output().unwrap()
    }

    fn binary(&self) -> PathBuf {
        self.path("home/bin/nv-0.1.0")
    }

    fn model(&self) -> PathBuf {
        self.path("home/nvdata/models/bge-small-en-v1.5/model.onnx")
    }

    fn leftovers(&self) -> Vec<String> {
        fs::read_dir(self.path("tmp"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn first_call_fetches_the_binary_and_runs_it_and_later_calls_fetch_nothing() {
    let world = World::new();

    let first = world.run("bin/nv", &["model", "info"]);

    assert!(first.status.success(), "{}", text(&first.stderr));
    assert_eq!(text(&first.stdout), "stub args: model info\n");
    assert!(
        text(&first.stderr).contains("downloading"),
        "{}",
        text(&first.stderr)
    );
    assert!(world.binary().is_file());
    assert_eq!(
        fs::metadata(world.binary()).unwrap().permissions().mode() & 0o111,
        0o111
    );
    // Gone from the server: a second call must not need it.
    world.unpublish_release();
    let second = world.run("bin/nv", &["search", "x"]);
    assert!(second.status.success(), "{}", text(&second.stderr));
    assert_eq!(text(&second.stdout), "stub args: search x\n");
    assert_eq!(text(&second.stderr), "");
}

#[test]
fn launcher_passes_stdin_and_the_exit_code_through() {
    let world = World::new();
    let mut child = world
        .command("bin/nv")
        .arg("add")
        .env("STUB_EXIT", "3")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"the body\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(3));
    assert_eq!(text(&output.stdout), "stub args: add\nthe body\n");
}

#[test]
fn launcher_does_not_fetch_the_model_but_ensure_does_and_prints_the_binary_path() {
    let world = World::new();

    world.run("bin/nv", &["date"]);
    assert!(
        !world.model().exists(),
        "the launcher fetches only the binary"
    );

    let ensure = world.run("scripts/ensure-nv.sh", &[]);

    assert!(ensure.status.success(), "{}", text(&ensure.stderr));
    // stdout is the path of the binary and nothing else: the hook builds JSON from it.
    assert_eq!(
        text(&ensure.stdout),
        format!("{}\n", world.binary().display())
    );
    assert_eq!(fs::read_to_string(world.model()).unwrap(), "the model");
}

#[test]
fn a_model_that_is_there_is_kept_and_not_fetched() {
    let world = World::new();
    let folder = world.model().parent().unwrap().to_path_buf();
    fs::create_dir_all(&folder).unwrap();
    fs::write(world.model(), "the user's own copy").unwrap();
    // A release without the model: asking for it would fail.
    fs::remove_file(world.path("release").join(MODEL_ASSET)).unwrap();
    let sums = fs::read_to_string(world.path("release/SHA256SUMS")).unwrap();
    let sums: String = sums
        .lines()
        .filter(|line| !line.contains("model-"))
        .map(|line| format!("{line}\n"))
        .collect();
    fs::write(world.path("release/SHA256SUMS"), sums).unwrap();

    let ensure = world.run("scripts/ensure-nv.sh", &[]);

    assert!(ensure.status.success(), "{}", text(&ensure.stderr));
    assert_eq!(
        fs::read_to_string(world.model()).unwrap(),
        "the user's own copy"
    );
}

#[test]
fn a_wrong_checksum_installs_nothing_and_leaves_no_temp_files() {
    let world = World::new();
    let sums = fs::read_to_string(world.path("release/SHA256SUMS")).unwrap();
    fs::write(
        world.path("release/SHA256SUMS"),
        sums.replacen(&sums[..8], "00000000", 1),
    )
    .unwrap();

    let output = world.run("scripts/ensure-nv.sh", &[]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).contains("checksum"),
        "{}",
        text(&output.stderr)
    );
    assert!(!world.binary().exists());
    assert!(!world.model().exists());
    assert_eq!(world.leftovers(), Vec::<String>::new());
}

#[test]
fn a_missing_asset_in_the_release_is_a_clear_error() {
    let world = World::new();
    fs::remove_file(world.path("release").join(ASSET)).unwrap();

    let output = world.run("bin/nv", &["date"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).contains(ASSET),
        "{}",
        text(&output.stderr)
    );
    assert_eq!(text(&output.stdout), "");
}

#[test]
fn with_downloads_off_and_nothing_installed_it_says_what_is_missing() {
    let world = World::new();

    let output = world
        .command("scripts/ensure-nv.sh")
        .env("NV_NO_DOWNLOAD", "1")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let said = text(&output.stderr);
    assert!(said.contains("not installed"), "{said}");
    assert!(said.contains("NV_NO_DOWNLOAD"), "{said}");
    assert!(!world.binary().exists());
}

#[test]
fn an_unsupported_platform_is_said_clearly() {
    let world = World::new();

    let output = world
        .command("bin/nv")
        .env("NV_TEST_UNAME", "Linux riscv64")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let said = text(&output.stderr);
    assert!(said.contains("no prebuilt binary"), "{said}");
    assert!(said.contains("riscv64"), "{said}");
    assert!(said.contains("README"), "{said}");
}

#[test]
fn macos_apple_silicon_asks_for_its_own_asset() {
    let world = World::new();

    // The release has only the Linux files.
    let output = world
        .command("bin/nv")
        .env("NV_TEST_UNAME", "Darwin arm64")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).contains("nv-0.1.0-aarch64-apple-darwin.tar.gz"),
        "{}",
        text(&output.stderr)
    );
}

#[test]
fn an_older_binary_is_removed_after_an_update() {
    let world = World::new();
    fs::create_dir_all(world.path("home/bin")).unwrap();
    fs::write(world.path("home/bin/nv-0.0.9"), "old").unwrap();

    let output = world.run("scripts/ensure-nv.sh", &["--binary"]);

    assert!(output.status.success(), "{}", text(&output.stderr));
    assert!(!world.path("home/bin/nv-0.0.9").exists());
    assert!(world.binary().exists());
}

#[test]
fn parallel_first_calls_download_the_binary_once() {
    let world = World::new();
    // A `curl` in front of the real one that logs what it fetches.
    let real = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|folder| folder.join("curl"))
        .find(|candidate| candidate.is_file())
        .expect("curl is installed");
    let log = world.path("curl.log");
    let stub = world.path("stub/curl");
    fs::write(
        &stub,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\nexec '{}' \"$@\"\n",
            log.display(),
            real.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        world.path("stub").display(),
        std::env::var("PATH").unwrap()
    );

    let children: Vec<_> = (0..4)
        .map(|_| {
            world
                .command("bin/nv")
                .arg("date")
                .env("PATH", &path)
                // The stub reads stdin until the end: an inherited, open stdin would hang.
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", text(&output.stderr));
        assert_eq!(text(&output.stdout), "stub args: date\n");
    }

    let fetched = fs::read_to_string(&log).unwrap();
    let asset_downloads = fetched.lines().filter(|line| line.contains(ASSET)).count();
    assert_eq!(asset_downloads, 1, "{fetched}");
}

#[test]
fn the_lock_of_an_installer_that_died_is_taken_over() {
    let world = World::new();
    let lock = world.path("home/bin/.install.lock");
    fs::create_dir_all(&lock).unwrap();
    // A process that has ended: its ID is free.
    let ended = Command::new("true").spawn().unwrap();
    let pid = ended.id();
    let _ = ended.wait_with_output();
    fs::write(lock.join("pid"), format!("{pid}\n")).unwrap();

    let output = world.run("scripts/ensure-nv.sh", &["--binary"]);

    assert!(output.status.success(), "{}", text(&output.stderr));
    assert!(world.binary().exists());
    assert!(!lock.exists(), "the lock is released at the end");
}
