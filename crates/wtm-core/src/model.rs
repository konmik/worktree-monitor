pub use crate::count_model::{Counts, LineCounts};
use std::fmt::{self, Write};
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

impl fmt::Display for Head {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Branch(name) => write!(f, "branch {name}"),
            Self::Detached(commit) => write!(f, "detached {commit}"),
            Self::Unborn(name) => write!(f, "unborn {name}"),
        }
    }
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

impl Snapshot {
    pub fn has_errors(&self) -> bool {
        self.config_error.is_some() || self.repos.iter().any(|repo| match &repo.worktrees {
            Err(_) => true,
            Ok(trees) => trees.iter().any(|tree| tree.result.is_err()),
        })
    }

    pub fn text(&self) -> String {
        let mut text = String::new();
        if let Some(error) = &self.config_error {
            let _ = writeln!(text, "Configuration error: {error}");
            if !self.repos.is_empty() {
                let _ = writeln!(text, "Using last good registrations.");
            }
        }
        if self.repos.is_empty() {
            let _ = writeln!(text, "No registered repositories.");
        }
        for repo in &self.repos {
            let _ = writeln!(text, "Repository: {}", repo.path.display());
            let _ = writeln!(text, "  Git directory: {}", repo.common_dir.display());
            match &repo.worktrees {
                Err(error) => { let _ = writeln!(text, "  ERROR: {error}"); }
                Ok(trees) if trees.is_empty() => { let _ = writeln!(text, "  No working trees (bare repository)."); }
                Ok(trees) => for tree in trees {
                    let _ = writeln!(text, "  {}", tree.path.display());
                    match &tree.result {
                        Ok(stats) => {
                            let _ = writeln!(text, "    {}  {}", stats.head, stats.counts);
                        }
                        Err(error) => { let _ = writeln!(text, "    ERROR: {error}"); }
                    }
                },
            }
        }
        text
    }
}
