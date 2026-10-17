#[path = "features/change-counting/worktree-counts-include-branch-changes/model.rs"]
mod count_model;
mod config;
mod git;
mod model;
#[path = "features/live-monitoring/poll.rs"]
mod poll;
#[path = "features/live-monitoring/process.rs"]
mod process;
#[path = "features/live-monitoring/ignored-directories-do-not-cause-background-scans/watch.rs"]
mod watch;
#[path = "features/live-monitoring/service.rs"]
mod monitoring_service;

pub use model::{Counts, Error, ErrorKind, Head, LineCounts, RepoEntry, Snapshot, WorktreeEntry, WorktreeStats};
pub use poll::{Monitor, MonitorUpdate, ScanActivity, ScanTarget};
pub use process::Cancel;

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Service {
    config_path: PathBuf,
}

impl Service {
    pub fn user() -> Result<Self, Error> {
        let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let home = std::env::var_os(variable)
            .filter(|home| !home.is_empty())
            .ok_or_else(|| Error::config(format!("{variable} is not set")))?;
        Ok(Self::at(PathBuf::from(home).join(".config/wtm/config.json")))
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { config_path: path.into() }
    }

    pub fn add(&self, path: &Path) -> Result<bool, Error> {
        config::update(&self.config_path, path, true, &Cancel::new())
    }

    pub fn remove(&self, path: &Path) -> Result<bool, Error> {
        config::update(&self.config_path, path, false, &Cancel::new())
    }

    pub fn snapshot(&self, cancel: &Cancel) -> Snapshot {
        match config::read(&self.config_path).and_then(|bytes| config::parse(&bytes)) {
            Ok(repos) => git::scan(&repos, None, cancel),
            Err(error) => Snapshot { repos: Vec::new(), config_error: Some(error) },
        }
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }
}

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod unit_tests;
