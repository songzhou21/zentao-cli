mod api;
mod browser;
mod bug;
mod cache;
mod cli;
mod config;
mod cookie_store;
mod search;
mod stats;
mod view;

use std::process::ExitCode;

fn main() -> ExitCode {
    match cli::run(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(cli::RunError::Clap(error)) => {
            let exit_code = error.exit_code();
            let _ = error.print();
            ExitCode::from(exit_code as u8)
        }
        Err(cli::RunError::Runtime(err)) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}
