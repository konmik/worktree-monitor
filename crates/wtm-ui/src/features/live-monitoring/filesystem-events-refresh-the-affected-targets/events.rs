use super::{Coordinator, PendingScan, normalize};
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
                self.passes.changed.extend(repo.worktrees.iter().cloned().map(|key| (key, PendingScan::Forced)));
            }
        }
        for key in batch.worktrees {
            let paths = batch.worktree_paths.get(&key);
            let candidate = !batch.force_worktrees.contains(&key) && paths.is_some_and(|paths| {
                !paths.is_empty() && paths.keys().all(|path| {
                    path.starts_with(&key.path) && path != &key.path
                        && path.file_name().is_none_or(|name| name != ".git" && name != ".gitignore" && name != ".gitattributes")
                })
            });
            let incoming = if candidate {
                PendingScan::Paths(paths.unwrap().keys().map(|path|
                    path.strip_prefix(&key.path).unwrap().to_path_buf()).collect())
            } else { PendingScan::Forced };
            match self.passes.changed.entry(key) {
                std::collections::hash_map::Entry::Occupied(mut current) => current.get_mut().merge(incoming),
                std::collections::hash_map::Entry::Vacant(current) => { current.insert(incoming); }
            }
        }
        if batch.global_excludes {
            self.passes.changed.extend(self.worktrees.keys().cloned().map(|key| (key, PendingScan::Forced)));
            self.discoveries.extend(self.repo_order.iter().cloned());
        }
        if batch.full {
            self.discoveries.extend(self.repo_order.iter().cloned());
            self.passes.changed.extend(self.worktrees.keys().cloned().map(|key| (key, PendingScan::Forced)));
        }
        if batch.topology || batch.full || !batch.plans.is_empty() { self.watch_dirty = true; }
    }
}
