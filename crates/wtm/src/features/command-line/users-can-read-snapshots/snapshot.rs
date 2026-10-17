use std::io::{self, Write};
use wtm_core::{Cancel, Service};

pub(super) fn print(service: &Service) -> io::Result<bool> {
    let snapshot = service.snapshot(&Cancel::new());
    io::stdout().lock().write_all(snapshot.text().as_bytes())?;
    Ok(!snapshot.has_errors())
}
