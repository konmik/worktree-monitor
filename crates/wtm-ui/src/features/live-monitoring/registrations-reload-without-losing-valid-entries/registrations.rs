use crate::{config, Error};
use std::path::PathBuf;

#[derive(Default)]
pub(crate) struct Registrations {
    pub repos: Vec<PathBuf>,
    pub error: Option<Error>,
    bytes: Option<Result<Vec<u8>, Error>>,
}

impl Registrations {
    pub(crate) fn reload(&mut self, bytes: Result<Vec<u8>, Error>) -> bool {
        if self.bytes.as_ref() == Some(&bytes) { return false; }
        match bytes.as_ref().map_err(Clone::clone).and_then(|bytes| config::parse(bytes)) {
            Ok(repos) => { self.repos = repos; self.error = None; }
            Err(error) => { self.error = Some(error); }
        }
        self.bytes = Some(bytes);
        true
    }
}
