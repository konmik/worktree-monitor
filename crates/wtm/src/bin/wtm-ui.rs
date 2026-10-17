#![cfg_attr(windows, windows_subsystem = "windows")]

#[path = "../features/dashboard-startup/dashboard-starts-with-user-service/startup.rs"]
mod startup;

fn main() -> std::process::ExitCode {
    startup::run()
}
