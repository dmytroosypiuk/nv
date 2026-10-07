//! Shared by the tests that run the plugin's shell scripts: a machine with nothing
//! installed, and a fake release to fetch from (a folder read through `file://`).
#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sha2::{Digest, Sha256};
use tempfile::TempDir;

pub const ASSET: &str = "nv-0.1.0-x86_64-unknown-linux-gnu.tar.gz";
pub const MODEL_ASSET: &str = "model-bge-small-en-v1.5.tar.gz";

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn sha256(path: &Path) -> String {
    let digest = Sha256::digest(fs::read(path).unwrap());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn tar(archive: &Path, folder: &Path, entries: &[&str]) {
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
pub struct World {
    pub dir: TempDir,
}

impl World {
    pub fn new() -> Self {
        let world = Self {
            dir: TempDir::new().unwrap(),
        };
        for folder in ["release", "home", "tmp", "stub"] {
            fs::create_dir(world.path(folder)).unwrap();
        }
        world.publish_release();
        world
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// The binary is a stub that echoes its arguments and stdin and exits as told.
    pub fn publish_release(&self) {
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

    pub fn unpublish_release(&self) {
        fs::remove_dir_all(self.path("release")).unwrap();
    }

    pub fn command(&self, script: &str) -> Command {
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

    pub fn run(&self, script: &str, args: &[&str]) -> Output {
        self.command(script).args(args).output().unwrap()
    }

    pub fn binary(&self) -> PathBuf {
        self.path("home/bin/nv-0.1.0")
    }

    pub fn model(&self) -> PathBuf {
        self.path("home/nvdata/models/bge-small-en-v1.5/model.onnx")
    }

    pub fn leftovers(&self) -> Vec<String> {
        fs::read_dir(self.path("tmp"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }
}

pub fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
