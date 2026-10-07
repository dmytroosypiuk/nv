use std::process::ExitCode;

fn main() -> ExitCode {
    match nv::cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("nv: {error:#}");
            ExitCode::FAILURE
        }
    }
}
