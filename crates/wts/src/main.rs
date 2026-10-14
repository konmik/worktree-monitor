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
    if arguments.live {
        launch_window()?;
        return Ok(true);
    }
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
        None => {
            let snapshot = service.snapshot(&Cancel::new());
            io::stdout().lock().write_all(snapshot.text().as_bytes())?;
            Ok(!snapshot.has_errors())
        }
    }
}

#[cfg(windows)]
fn launch_window() -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use std::process::{Command, Stdio};
    use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};

    for handle in [io::stdin().as_raw_handle(), io::stdout().as_raw_handle(), io::stderr().as_raw_handle()] {
        if !handle.is_null() && handle as isize != -1 && unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }

    let executable = std::env::current_exe()?.with_file_name("wts-ui.exe");
    Command::new(&executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| io::Error::new(error.kind(), format!("Could not launch {}: {error}", executable.display())))?;
    Ok(())
}

#[cfg(not(windows))]
fn launch_window() -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "--live is supported only on Windows"))
}

fn main() -> ExitCode {
    match run(Arguments::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => { eprintln!("Error: {error}"); ExitCode::FAILURE }
    }
}
