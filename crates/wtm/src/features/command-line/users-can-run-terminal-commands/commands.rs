#[path = "../users-can-register-repositories/commands.rs"]
mod registration;

use clap::Parser;
use registration::Registration;
use std::process::ExitCode;
use wtm_core::Service;

#[derive(Parser)]
#[command(version, about = "Track changed lines across Git repositories and worktrees", arg_required_else_help = true, args_conflicts_with_subcommands = true)]
struct Arguments {
    #[command(subcommand)]
    command: Option<Registration>,
    #[arg(long, exclusive = true, help = "Show the live dashboard in this terminal")]
    live: bool,
}

fn execute(arguments: Arguments) -> Result<(), Box<dyn std::error::Error>> {
    if arguments.live { return wtm_terminal::run(); }
    if let Some(command) = arguments.command {
        let service = Service::user()?;
        return registration::execute(&service, command);
    }
    unreachable!()
}

pub(crate) fn run() -> ExitCode {
    match execute(Arguments::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => { eprintln!("Error: {error}"); ExitCode::FAILURE }
    }
}
