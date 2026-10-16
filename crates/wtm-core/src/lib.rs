mod config;
mod git;
mod model;
mod poll;
mod process;
mod watch;

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

    pub fn monitor(self) -> Result<Monitor, Error> {
        Monitor::start(self, None)
    }

    pub fn monitor_from(self, launch_directory: PathBuf) -> Result<Monitor, Error> {
        Monitor::start(self, Some(launch_directory))
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }
}

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod unit_tests;
