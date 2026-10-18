use super::Coordinator;
use crate::{config, Error, ErrorKind};
use std::time::Instant;

impl Coordinator {
    pub(super) fn resolve_launch(&mut self) -> bool {
        let worked = self.launch_changed && self.launch_directory.is_some();
        if self.launch_changed {
            self.launch_changed = false;
            if let Some(directory) = &self.launch_directory {
                let start = Instant::now();
                let result = config::resolve(directory, &self.cancel);
                self.polling_time.record(start, Instant::now());
                match result {
                    Ok(common) => { self.launch_common = Some(common); self.launch_error = None; }
                    Err(error) if is_not_git(&error) => { self.launch_error = None; }
                    Err(error) => { self.launch_error = Some(error); }
                }
                self.refresh_repositories();
            }
        }
        worked
    }
}

fn is_not_git(error: &Error) -> bool {
    error.kind == ErrorKind::Git && error.message.starts_with("fatal: not a git repository")
}
