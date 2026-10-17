#[path = "../fatal-startup-errors-are-visible/errors.rs"]
mod errors;

use std::process::ExitCode;

fn start() -> Result<(), Box<dyn std::error::Error>> {
    let service = wtm_core::Service::user()?;
    window_win::run(service, std::env::current_dir()?)?;
    Ok(())
}


pub(crate) fn run() -> ExitCode {
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => errors::report(error.as_ref()),
    }
}
