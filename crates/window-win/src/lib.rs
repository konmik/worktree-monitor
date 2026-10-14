pub mod dashboard;

#[cfg(windows)]
mod native;

#[cfg(windows)]
pub use native::run;

#[cfg(not(windows))]
pub fn run(_service: wts_core::Service) -> Result<(), std::io::Error> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "--live is supported only on Windows"))
}
