mod api;
mod browser;
mod bug;
mod cli;
mod config;
mod cookie_store;
mod search;

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
