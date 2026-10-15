use clap::{Parser, Subcommand};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use wts_core::{Cancel, Service};

#[derive(Parser)]
#[command(version, args_conflicts_with_subcommands = true, about = "Track changed lines across Git repositories and worktrees")]
struct Arguments {
    #[arg(long, help = "Open the always-on-top Windows status window")]
    live: bool,
    #[command(subcommand)]
    command: Option<Registration>,
}

#[derive(Subcommand)]
enum Registration {
    #[command(about = "Register a repository from any worktree or subdirectory")]
    Add { path: PathBuf },
    #[command(about = "Unregister a repository")]
    Remove { path: PathBuf },
}

fn run(arguments: Arguments) -> Result<bool, Box<dyn std::error::Error>> {
    let service = Service::user()?;
    match arguments.command {
        Some(Registration::Add { path }) => {
            let changed = service.add(&path)?;
            println!("{}", if changed { "Repository registered." } else { "Repository already registered." });
            Ok(true)
        }
        Some(Registration::Remove { path }) => {
            let changed = service.remove(&path)?;
            println!("{}", if changed { "Repository removed." } else { "Repository is not registered." });
            Ok(true)
        }
        None if arguments.live => { window_win::run(service, std::env::current_dir()?)?; Ok(true) }
        None => {
            let snapshot = service.snapshot(&Cancel::new());
            io::stdout().lock().write_all(snapshot.text().as_bytes())?;
            Ok(!snapshot.has_errors())
        }
    }
}

fn main() -> ExitCode {
    match run(Arguments::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => { eprintln!("Error: {error}"); ExitCode::FAILURE }
    }
}
