use crate::{config, git, Cancel, Service, Snapshot};

impl Service {
    pub fn snapshot(&self, cancel: &Cancel) -> Snapshot {
        match config::read(&self.config_path).and_then(|bytes| config::parse(&bytes)) {
            Ok(repos) => git::scan(&repos, None, cancel),
            Err(error) => Snapshot { repos: Vec::new(), config_error: Some(error) },
        }
    }
}
