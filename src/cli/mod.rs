//! Command line: object first, then action.

use clap::{CommandFactory, Parser};

/// A local, offline knowledge store for Claude Code.
#[derive(Debug, Parser)]
#[command(name = "nv", version)]
pub struct Cli {}

/// Runs the command given on the command line.
pub fn run() -> anyhow::Result<()> {
    Cli::parse();
    // No commands yet: show what nv is.
    Cli::command().print_help()?;
    Ok(())
}
