#[path = "../users-can-register-repositories/commands.rs"]
mod registration;

use clap::Parser;
use registration::Registration;
use std::process::ExitCode;
use wtm_ui::Service;

#[derive(Parser)]
#[command(version, about = "Track changed lines across Git repositories and worktrees", arg_required_else_help = true)]
struct Arguments {
    #[command(subcommand)]
    command: Registration,
}

fn execute(arguments: Arguments) -> Result<(), Box<dyn std::error::Error>> {
    let service = Service::user()?;
    registration::execute(&service, arguments.command)
}

pub(crate) fn run() -> ExitCode {
    match execute(Arguments::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => { eprintln!("Error: {error}"); ExitCode::FAILURE }
    }
}
