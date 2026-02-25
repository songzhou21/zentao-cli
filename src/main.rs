mod api;
mod browser;
mod bug;
mod cli;
mod config;

use std::process::ExitCode;

fn main() -> ExitCode {
    match cli::run(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}
