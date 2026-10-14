pub mod dashboard;
pub mod order;
pub mod placement;
pub mod visibility;

#[cfg(windows)]
mod placement_win;

#[cfg(windows)]
mod native;

#[cfg(windows)]
pub use native::run;

#[cfg(not(windows))]
pub fn run(_service: wts_core::Service, _launch_directory: std::path::PathBuf) -> Result<(), std::io::Error> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "--live is supported only on Windows"))
}
