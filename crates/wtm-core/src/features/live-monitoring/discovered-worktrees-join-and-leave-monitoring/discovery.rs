use super::{Coordinator, WorktreeKey, WorktreeState};
use crate::{git, Error};
use std::collections::HashSet;
use std::path::Path;

impl Coordinator {
    pub(super) fn apply_discovery(&mut self, common: &Path, result: Result<git::DiscoveredRepo, Error>) {
        let repo = self.repos.get_mut(common).unwrap();
        let recovered = matches!(repo.discovered, Some(Err(_)));
        if let Ok(discovered) = &result {
            repo.path = discovered.path.clone();
            let mut keys = Vec::new();
            for tree in &discovered.trees {
                let key = WorktreeKey::new(&repo.common_dir, &tree.path);
                if keys.contains(&key) { continue; }
                keys.push(key.clone());
                if let Some(state) = self.worktrees.get_mut(&key) {
                    state.path = tree.path.clone();
                    if let Some(error) = &tree.error {
                        state.last = Some(Err(error.clone()));
                    } else if recovered || state.last.as_ref().is_some_and(Result::is_err) {
                        self.passes.changed.insert(key);
                    }
                } else {
                    self.worktrees.insert(key.clone(), WorktreeState {
                        path: tree.path.clone(), last: tree.error.clone().map(Err),
                        activity: None,
                    });
                    if tree.error.is_none() { self.passes.changed.insert(key); }
                }
            }
            let retained: HashSet<_> = keys.iter().cloned().collect();
            self.worktrees.retain(|key, _| key.common != common || retained.contains(key));
            self.passes.changed.retain(|key| key.common != common || retained.contains(key));
            repo.worktrees = keys;
            if let Some(tree) = discovered.trees.iter().find(|tree| tree.error.is_none()) {
                if let Ok(sources) = git::global_sources(&tree.path, &self.cancel) { repo.sources = sources; }
            }
        }
        repo.discovered = Some(result);
        self.watch_dirty = true;
    }
}
