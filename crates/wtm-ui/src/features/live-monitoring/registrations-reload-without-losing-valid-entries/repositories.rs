use crate::config;
use super::{Coordinator, normalize, RepoState};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn display_path(common: &Path) -> PathBuf {
    common.parent().filter(|_| common.file_name().is_some_and(|name| name == ".git"))
        .unwrap_or(common).to_path_buf()
}

impl Coordinator {
    pub(super) fn reload_registrations(&mut self) {
        if self.config_changed {
            self.config_changed = false;
            if self.registrations.reload(config::read(&self.service.config_path)) {
                self.refresh_repositories();
            }
        }
    }

    pub(super) fn refresh_repositories(&mut self) {
        let mut desired = Vec::new();
        let mut seen = HashSet::new();
        for common in self.registrations.repos.iter().chain(self.launch_common.iter()) {
            let key = normalize(common);
            if seen.insert(key.clone()) { desired.push((key, common.clone())); }
        }
        self.repos.retain(|key, _| seen.contains(key));
        self.worktrees.retain(|key, _| seen.contains(&key.common));
        self.passes.changed.retain(|key| seen.contains(&key.common));
        self.discoveries.retain(|key| seen.contains(key));
        self.repo_order = desired.iter().map(|(key, _)| key.clone()).collect();
        for (key, common_dir) in desired {
            if let Some(repo) = self.repos.get_mut(&key) {
                repo.common_dir = common_dir;
            } else {
                self.discoveries.insert(key.clone());
                self.repos.insert(key, RepoState {
                    path: display_path(&common_dir), common_dir,
                    discovered: None, worktrees: Vec::new(), sources: Vec::new(),
                    activity: None,
                });
            }
        }
        self.watch_dirty = true;
    }
}
