#[path = "../../repository-registration/users-can-register-and-remove-repositories/registration.rs"]
mod registry_writer;

use clap::Subcommand;
use wtm_ui::Cancel;
use std::path::PathBuf;
use wtm_ui::Service;

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
            let changed = registry_writer::update(service.config_path(), &path, true, &Cancel::new())?;
            println!("{}", if changed { "Repository registered." } else { "Repository already registered." });
            Ok(())
        }
        Registration::Remove { path } => {
            let changed = registry_writer::update(service.config_path(), &path, false, &Cancel::new())?;
            println!("{}", if changed { "Repository removed." } else { "Repository is not registered." });
            Ok(())
        }
    }
}
