#![cfg_attr(windows, windows_subsystem = "windows")]

use std::process::ExitCode;

#[cfg(windows)]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let service = wts_core::Service::user()?;
    window_win::run(service, std::env::current_dir()?)?;
    Ok(())
}

#[cfg(not(windows))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    Err("wts-ui is supported only on Windows".into())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let message = format!("Error: {error}");
            #[cfg(windows)]
            {
                use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
                let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
                let title: Vec<u16> = "wts-ui".encode_utf16().chain(Some(0)).collect();
                unsafe { MessageBoxW(std::ptr::null_mut(), message.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR); }
            }
            #[cfg(not(windows))]
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
