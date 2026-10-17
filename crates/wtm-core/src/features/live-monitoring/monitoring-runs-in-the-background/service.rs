use crate::{Error, Monitor, Service};
use std::path::PathBuf;

impl Service {
    pub fn monitor(self) -> Result<Monitor, Error> {
        Monitor::start(self, None)
    }

    pub fn monitor_from(self, launch_directory: PathBuf) -> Result<Monitor, Error> {
        Monitor::start(self, Some(launch_directory))
    }
}
