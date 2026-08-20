mod login;
pub(crate) mod profile;
pub(crate) mod status;

use crate::cli::GlobalArgs;
use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub(crate) struct AuthArgs {
    #[command(subcommand)]
    pub(crate) command: AuthSubCommands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum AuthSubCommands {
    Login(login::LoginArgs),
    Status(status::AuthStatusArgs),
    SelectChromeProfile(profile::ProfileArgs),
}

pub(crate) fn run(args: AuthArgs, global: &GlobalArgs) -> Result<()> {
    match args.command {
        AuthSubCommands::Login(args) => login::run(args, global),
        AuthSubCommands::Status(args) => status::run(args, global),
        AuthSubCommands::SelectChromeProfile(args) => profile::run(args, global),
    }
}
