#[path = "../fatal-startup-errors-are-visible/errors.rs"]
mod errors;
#[cfg(windows)]
#[path = "../gui-app-can-only-start-once/instance.rs"]
mod instance;

use std::process::ExitCode;

fn start() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    let Some(_instance) = instance::acquire()? else {
        return Ok(());
    };
    let service = wtm_ui::Service::user()?;
    crate::window::run(service, std::env::current_dir()?)?;
    Ok(())
}


pub fn run() -> ExitCode {
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => errors::report(error.as_ref()),
    }
}
