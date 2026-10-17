use super::{Coordinator, normalize};
use crate::watch;
use std::collections::HashSet;
use std::time::Instant;

impl Coordinator {
    pub(super) fn receive_watch(&mut self) {
        let batch = self.watch.drain(Instant::now());
        if !batch.is_empty() { self.enqueue(batch); }
    }

    pub(super) fn enqueue(&mut self, batch: watch::WatchBatch) {
        if batch.config || batch.full { self.config_changed = true; }
        if batch.launch || batch.full { self.launch_changed = true; }
        let discoveries: HashSet<_> = batch.discoveries.iter().map(|common| normalize(common)).collect();
        self.discoveries.extend(discoveries.iter().cloned());
        for common in batch.repositories {
            let common = normalize(&common);
            if discoveries.contains(&common) { continue; }
            if let Some(repo) = self.repos.get(&common) {
                self.passes.changed.extend(repo.worktrees.iter().cloned());
            }
        }
        self.passes.changed.extend(batch.worktrees);
        if batch.global_excludes {
            self.passes.changed.extend(self.worktrees.keys().cloned());
            self.discoveries.extend(self.repo_order.iter().cloned());
        }
        if batch.full {
            self.discoveries.extend(self.repo_order.iter().cloned());
            self.passes.changed.extend(self.worktrees.keys().cloned());
        }
        if batch.topology || batch.full || !batch.plans.is_empty() { self.watch_dirty = true; }
    }
}
