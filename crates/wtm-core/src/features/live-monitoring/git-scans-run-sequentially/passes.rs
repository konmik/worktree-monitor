use super::Coordinator;
use crate::git;
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
            let start = Instant::now();
            self.start_scan(&common, None);
            let result = git::discover(&self.repos[&common].common_dir, &self.cancel);
            self.apply_discovery(&common, result);
            self.polling_time.record(start, Instant::now());
            self.finish_scan();
            self.publish();
        }
        self.reconcile_watches();
        self.receive_watch();
        let changed = std::mem::take(&mut self.passes.changed);
        let keys: Vec<_> = self.repo_order.iter().flat_map(|common| self.repos[common].worktrees.iter())
            .filter(|key| changed.contains(*key)).cloned().collect();
        for key in keys {
            if self.cancel.is_cancelled() { return; }
            if !self.repos.get(&key.common).is_some_and(|repo| matches!(repo.discovered, Some(Ok(_)))) { continue; }
            let Some(state) = self.worktrees.get(&key) else { continue; };
            let path = state.path.clone();
            worked = true;
            let start = Instant::now();
            self.start_scan(&key.common, Some(&key));
            let result = git::scan_tree(&path, &self.cancel);
            self.polling_time.record(start, Instant::now());
            self.worktrees.get_mut(&key).unwrap().last = Some(result);
            self.finish_scan();
            self.publish();
            self.receive_watch();
        }
        if !self.cancel.is_cancelled() { self.reconcile_watches(); }
        if worked { self.passes.finish(Instant::now()); }
    }
}
