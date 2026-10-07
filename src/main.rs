use std::env;
use std::io;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use nv::cli::{self, Cli, Context};
use nv::clock::Now;
use nv::config::NvHome;
use nv::knowledge::change_log::Actor;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match context().and_then(|context| {
        cli::run(
            cli,
            &context,
            &mut io::stdin().lock(),
            &mut io::stdout().lock(),
        )
    }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("nv: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// The only place that reads the environment and the clock.
fn context() -> Result<Context> {
    let run_by_claude_code = env::var_os("CLAUDECODE").is_some_and(|value| value == "1");
    Ok(Context {
        nv_home: NvHome::resolve(env::var_os("NV_HOME"), env::home_dir())?,
        actor: Actor::from_env(env::var("NV_ACTOR").ok().as_deref(), run_by_claude_code)?,
        // NV_NOW fixes the clock in tests.
        now: match env::var("NV_NOW") {
            Ok(time) => Now::parse(&time)?,
            Err(_) => Now::from_system_clock(),
        },
    })
}
