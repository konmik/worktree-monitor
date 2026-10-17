use clap::Subcommand;
use std::path::PathBuf;
use wtm_core::Service;

#[derive(Subcommand)]
pub(super) enum Registration {
    #[command(about = "Register a repository from any worktree or subdirectory")]
    Add { path: PathBuf },
    #[command(about = "Unregister a repository")]
    Remove { path: PathBuf },
}

pub(super) fn execute(service: &Service, command: Registration) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Registration::Add { path } => {
            let changed = service.add(&path)?;
            println!("{}", if changed { "Repository registered." } else { "Repository already registered." });
            Ok(())
        }
        Registration::Remove { path } => {
            let changed = service.remove(&path)?;
            println!("{}", if changed { "Repository removed." } else { "Repository is not registered." });
            Ok(())
        }
    }
}
