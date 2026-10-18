use crate::{Error, Monitor};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Service {
    pub(crate) config_path: PathBuf,
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

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn monitor(self) -> Result<Monitor, Error> {
        Monitor::start(self, None)
    }

    pub fn monitor_from(self, launch_directory: PathBuf) -> Result<Monitor, Error> {
        Monitor::start(self, Some(launch_directory))
    }
}
