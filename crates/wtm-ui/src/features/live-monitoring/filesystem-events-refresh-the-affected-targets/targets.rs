use super::{Coordinator, normalize};
use crate::{git, watch};
use std::time::Instant;

impl Coordinator {
    pub(super) fn watch_targets(&self) -> watch::WatchTargets {
        let mut global_excludes = git::global_source_candidates();
        for repo in self.repos.values() { global_excludes.extend(repo.sources.global.iter().cloned()); }
        global_excludes.sort();
        global_excludes.dedup();
        let repository_sources = self.repos.values().flat_map(|repo| repo.sources.repository.iter()
            .filter(|source| !global_excludes.iter().any(|global| normalize(global) == normalize(source)))
            .map(|source| (repo.common_dir.clone(), source.clone()))).collect();
        watch::WatchTargets {
            config: self.service.config_path.clone(),
            launch: self.launch_directory.clone(),
            repositories: self.repo_order.iter().map(|key| self.repos[key].common_dir.clone()).collect(),
            worktrees: self.worktrees.iter().map(|(key, state)| (key.clone(), state.path.clone())).collect(),
            global_excludes,
            repository_sources,
        }
    }

    pub(super) fn reconcile_watches(&mut self) {
        let now = Instant::now();
        if !self.watch_dirty && !self.watch.next_retry().is_some_and(|retry| now >= retry) { return; }
        let status = self.watch.reconcile(&self.watch_targets(), now);
        self.watch_dirty = false;
        self.recover_watch_status(status);
    }
}
