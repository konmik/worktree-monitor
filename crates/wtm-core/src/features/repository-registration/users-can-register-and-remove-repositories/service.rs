use crate::{config, Cancel, Error};
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

    pub fn add(&self, path: &Path) -> Result<bool, Error> {
        config::update(&self.config_path, path, true, &Cancel::new())
    }

    pub fn remove(&self, path: &Path) -> Result<bool, Error> {
        config::update(&self.config_path, path, false, &Cancel::new())
    }


    pub fn config_path(&self) -> &Path {
        &self.config_path
    }
}
