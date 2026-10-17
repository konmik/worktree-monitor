pub use crate::count_model::{Counts, LineCounts};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Config,
    Io,
    Git,
    InvalidOutput,
    Conflict,
    Timeout,
    Cancelled,
    OutputLimit,
    ChangedDuringScan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }

    pub(crate) fn config(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Config, message)
    }

    pub(crate) fn io(context: impl fmt::Display, error: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, format!("{context}: {error}"))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Head {
    Branch(String),
    Detached(String),
    Unborn(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeStats {
    pub head: Head,
    pub counts: Counts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    pub result: Result<WorktreeStats, Error>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepoEntry {
    pub common_dir: PathBuf,
    pub path: PathBuf,
    pub worktrees: Result<Vec<WorktreeEntry>, Error>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Snapshot {
    pub repos: Vec<RepoEntry>,
    pub config_error: Option<Error>,
}
