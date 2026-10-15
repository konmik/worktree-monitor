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
    #[arg(long, hide = true, requires = "live")]
    live_child: bool,
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
    #[cfg(windows)]
    if arguments.live && !arguments.live_child {
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
        None if arguments.live => { window_win::run(service, std::env::current_dir()?)?; Ok(true) }
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
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetHandleInformation(handle: *mut std::ffi::c_void, mask: u32, flags: u32) -> i32;
    }

    for handle in [io::stdin().as_raw_handle(), io::stdout().as_raw_handle(), io::stderr().as_raw_handle()] {
        if !handle.is_null() && handle as isize != -1 && unsafe { SetHandleInformation(handle, 1, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }

    Command::new(std::env::current_exe()?)
        .args(["--live", "--live-child"])
        .creation_flags(0x00000008)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

fn main() -> ExitCode {
    match run(Arguments::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => { eprintln!("Error: {error}"); ExitCode::FAILURE }
    }
}

#[cfg(test)]
mod tests {
    use super::Arguments;
    use clap::Parser;

    #[test]
    fn window_host_requires_live_mode() {
        assert!(Arguments::try_parse_from(["wts", "--live-child"]).is_err());
        let host = Arguments::try_parse_from(["wts", "--live", "--live-child"]).unwrap();
        assert!(host.live);
        assert!(host.live_child);
        assert!(host.command.is_none());
        assert!(Arguments::try_parse_from(["wts", "--live", "--live-child", "add", "."]).is_err());
    }
}
