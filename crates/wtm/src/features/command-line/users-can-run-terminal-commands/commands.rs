#[path = "../users-can-register-repositories/commands.rs"]
mod registration;
#[path = "../users-can-read-snapshots/snapshot.rs"]
mod snapshot;

use clap::Parser;
use registration::Registration;
use std::process::ExitCode;
use wtm_core::Service;

#[derive(Parser)]
#[command(version, about = "Track changed lines across Git repositories and worktrees")]
struct Arguments {
    #[command(subcommand)]
    command: Option<Registration>,
}

fn execute(arguments: Arguments) -> Result<bool, Box<dyn std::error::Error>> {
    let service = Service::user()?;
    match arguments.command {
        Some(command) => {
            registration::execute(&service, command)?;
            Ok(true)
        }
        None => Ok(snapshot::print(&service)?),
    }
}

pub(crate) fn run() -> ExitCode {
    match execute(Arguments::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => { eprintln!("Error: {error}"); ExitCode::FAILURE }
    }
}
