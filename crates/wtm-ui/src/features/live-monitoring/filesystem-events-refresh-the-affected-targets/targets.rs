use super::Coordinator;
use crate::{git, watch};
use std::time::Instant;

impl Coordinator {
    pub(super) fn watch_targets(&self) -> watch::WatchTargets {
        let mut global_excludes = git::global_source_candidates();
        for repo in self.repos.values() { global_excludes.extend(repo.sources.iter().cloned()); }
        global_excludes.sort();
        global_excludes.dedup();
        watch::WatchTargets {
            config: self.service.config_path.clone(),
            launch: self.launch_directory.clone(),
            repositories: self.repo_order.iter().map(|key| self.repos[key].common_dir.clone()).collect(),
            worktrees: self.worktrees.iter().map(|(key, state)| (key.clone(), state.path.clone())).collect(),
            global_excludes,
        }
    }

    pub(super) fn reconcile_watches(&mut self) {
        let now = Instant::now();
        if !self.watch_dirty && !self.watch.next_retry().is_some_and(|retry| now >= retry) { return; }
        let deferred = self.worktrees.values().any(|state| state.last.is_none());
        let status = if deferred {
            self.watch.prepare(&self.watch_targets(), now)
        } else {
            self.watch.reconcile(&self.watch_targets(), now)
        };
        self.watch_dirty = deferred;
        self.recover_watch_status(status);
    }
}
