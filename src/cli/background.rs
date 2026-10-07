//! Embedding in the background, so `nv add` returns at once.

use std::env;
use std::process::{Command, Stdio};

use anyhow::Result;

pub trait BackgroundEmbedding {
    /// Starts embedding the pending notes and returns without waiting.
    fn start(&mut self) -> Result<()>;
}

/// Starts a detached `nv model embed-pending` process.
pub struct DetachedEmbedder;

impl BackgroundEmbedding for DetachedEmbedder {
    fn start(&mut self) -> Result<()> {
        let mut command = Command::new(env::current_exe()?);
        command
            .args(["model", "embed-pending"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Its own process group: it outlives the shell command that ran `nv add`.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        }
        command.spawn()?;
        Ok(())
    }
}
