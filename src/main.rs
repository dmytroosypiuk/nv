use std::env;
use std::io;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use nv::cli::{self, Cli, Context, DetachedEmbedder, Streams};
use nv::clock::Now;
use nv::config::NvHome;
use nv::knowledge::change_log::Actor;
use nv::search::bge::BgeSmallLoader;

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("nv: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// The only place that reads the environment and the clock.
fn run(cli: Cli) -> Result<()> {
    let run_by_claude_code = env::var_os("CLAUDECODE").is_some_and(|value| value == "1");
    let nv_home = NvHome::resolve(env::var_os("NV_HOME"), env::home_dir())?;
    let mut loader = BgeSmallLoader::new(nv_home.model_dir());
    let mut context = Context {
        nv_home,
        actor: Actor::from_env(env::var("NV_ACTOR").ok().as_deref(), run_by_claude_code)?,
        // NV_NOW fixes the clock in tests.
        now: match env::var("NV_NOW") {
            Ok(time) => Now::parse(&time)?,
            Err(_) => Now::from_system_clock(),
        },
        loader: &mut loader,
        background: &mut DetachedEmbedder,
    };
    let streams = Streams {
        stdin: &mut io::stdin().lock(),
        out: &mut io::stdout().lock(),
        err: &mut io::stderr().lock(),
    };
    cli::run(cli, &mut context, streams)
}
