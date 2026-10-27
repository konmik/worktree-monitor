use super::{Coordinator, PendingScan, WorktreeKey};
use crate::git;
use std::collections::HashSet;
use std::time::Instant;

impl Coordinator {
    pub(super) fn scan_pass(&mut self) {
        let mut worked = false;
        worked |= self.resolve_launch();
        let discoveries = std::mem::take(&mut self.discoveries);
        for common in self.repo_order.clone() {
            if self.cancel.is_cancelled() { return; }
            if !discoveries.contains(&common) { continue; }
            worked = true;
            self.start_scan(&common, None);
            let reading = self.polling_time.start();
            let result = git::discover(&self.repos[&common].common_dir, &self.cancel);
            self.apply_discovery(&common, result);
            drop(reading);
            self.finish_scan();
            self.publish();
        }
        self.reconcile_watches();
        self.receive_watch();
        let ready: HashSet<_> = self.worktrees.iter().filter_map(|(key, state)|
            state.last.is_some().then_some(key.clone())).collect();
        worked |= self.rebuild_ready_plans(&ready);
        self.receive_watch();
        let changed = std::mem::take(&mut self.passes.changed);
        let keys: Vec<_> = self.repo_order.iter().flat_map(|common| self.repos[common].worktrees.iter())
            .filter(|key| changed.contains_key(*key)).cloned().collect();
        for key in keys {
            if self.cancel.is_cancelled() { return; }
            if !self.repos.get(&key.common).is_some_and(|repo| matches!(repo.discovered, Some(Ok(_)))) { continue; }
            let Some(state) = self.worktrees.get(&key) else { continue; };
            let path = state.path.clone();
            worked = true;
            let reading = self.polling_time.start();
            let relevant = match &changed[&key] {
                PendingScan::Forced => Ok(true),
                PendingScan::Paths(paths) => git::relevant_watch_paths(&path, paths, &self.cancel),
            };
            if matches!(relevant, Ok(false)) {
                continue;
            }
            self.start_scan(&key.common, Some(&key));
            let result = relevant.and_then(|_| git::scan_tree(&path, &self.cancel));
            drop(reading);
            self.worktrees.get_mut(&key).unwrap().last = Some(result);
            self.finish_scan();
            self.publish();
            self.receive_watch();
        }
        if !self.cancel.is_cancelled() { self.reconcile_watches(); }
        let newly_ready: HashSet<_> = self.worktrees.iter().filter_map(|(key, state)|
            (state.last.is_some() && !ready.contains(key)).then_some(key.clone())).collect();
        worked |= self.rebuild_ready_plans(&newly_ready);
        if worked { self.passes.finish(Instant::now()); }
    }

    fn rebuild_ready_plans(&mut self, ready: &HashSet<WorktreeKey>) -> bool {
        if self.cancel.is_cancelled() || ready.is_empty() || !self.watch.has_pending_plans() {
            return false;
        }
        let start = Instant::now();
        let reading = self.polling_time.start();
        let status = self.watch.rebuild_plans(&self.watch_targets(), start, ready);
        drop(reading);
        self.recover_watch_status(status);
        true
    }
}
