#[path = "features/command-line/users-can-run-terminal-commands/commands.rs"]
mod commands;

fn main() -> std::process::ExitCode {
    commands::run()
}
