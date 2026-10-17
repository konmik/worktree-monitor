use super::{absolute, normalize, topology_event, rule_file, WorktreeKey, WatchBoundary, WatchTargets, WatchBatch, PlanPhase, plan_for, fallback_plan, WorktreeWatchPlan};
use crate::git;
use notify::{Event, EventKind};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct EventPolicy {
    pub(super) worktrees: Vec<WorktreeEventPolicy>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorktreeEventPolicy {
    pub(super) key: WorktreeKey,
    pub(super) root: PathBuf,
    pub(super) plan: Arc<WorktreeWatchPlan>,
    pub(super) protected: Vec<PathBuf>,
}

impl EventPolicy {
    pub(super) fn filter(&self, mut event: Event) -> Option<Event> {
        if event.need_rescan() || event.paths.is_empty() {
            return Some(event);
        }
        event.paths.retain(|path| self.allows(path, &event.kind));
        (!event.paths.is_empty()).then_some(event)
    }

    pub(super) fn allows(&self, path: &Path, kind: &EventKind) -> bool {
        let path = normalize(&absolute(path));
        let mut matching = self.worktrees.iter().filter(|worktree| path.starts_with(&worktree.root)).peekable();
        matching.peek().is_none() || matching.any(|worktree| !ignored_by_plan(&path, kind, worktree))
    }
}

fn ignored_by_plan(path: &Path, kind: &EventKind, worktree: &WorktreeEventPolicy) -> bool {
    if worktree.protected.iter().any(|protected| {
        path == protected || path.starts_with(protected)
            || topology_event(kind) && protected.starts_with(path)
    }) {
        return false;
    }
    if !worktree.plan.ignored_directories.iter().any(|directory| {
        path == directory || path.starts_with(directory)
    }) {
        return false;
    }
    if worktree.plan.tracked_files.contains(path)
        || topology_event(kind) && worktree.plan.tracked_directories.contains(path)
        || rule_file(path) && path.parent().is_some_and(|parent| worktree.plan.tracked_directories.contains(parent)) {
        return false;
    }
    true
}

impl WatchBoundary {
    pub(super) fn refresh_plans(&mut self, targets: &WatchTargets) {
        let desired: HashSet<_> = targets.worktrees.iter().map(|(key, _)| key.clone()).collect();
        self.plans.retain(|key, _| desired.contains(key));
        self.plan_dirty.retain(|key| desired.contains(key));
        self.plan_errors.retain(|key| desired.contains(key));
        for (key, path) in &targets.worktrees {
            if self.plans.contains_key(key) && !self.plan_dirty.contains(key) && !self.plan_errors.contains(key) { continue; }
            self.refresh_plan(key, path);
        }
    }

    pub(super) fn refresh_plan(&mut self, key: &WorktreeKey, path: &Path) {
        #[cfg(test)]
        if let Some(before_plan) = &mut self.before_plan { before_plan(); }
        let plan = git::watch_paths(path, &self.cancel).map(|paths| plan_for(path, &paths));
        match plan {
            Ok(plan) => {
                self.plans.insert(key.clone(), Arc::new(plan));
                self.plan_errors.remove(key);
            }
            Err(_) => {
                self.plans.insert(key.clone(), Arc::new(fallback_plan(path)));
                self.plan_errors.insert(key.clone());
            }
        }
        self.plan_dirty.remove(key);
    }

    pub(super) fn refresh_dirty_plans(&mut self) {
        if self.plan_phase == PlanPhase::Deferred { return; }
        let dirty: Vec<_> = self.plan_dirty.iter().cloned().collect();
        if dirty.is_empty() { return; }
        for key in dirty { self.refresh_plan(&key, &key.path); }
        self.publish_plan_policy();
    }

    pub(super) fn publish_policy(&self, targets: &WatchTargets) {
        let mut worktrees = Vec::new();
        for (key, path) in &targets.worktrees {
            let Some(plan) = self.plans.get(key) else { continue; };
            let mut protected = vec![normalize(&absolute(&path.join(".git"))), key.common.clone()];
            protected.extend(targets.repositories.iter().cloned());
            protected.extend(targets.global_excludes.iter().cloned());
            protected.push(targets.config.clone());
            if let Some(launch) = &targets.launch {
                protected.extend(launch.ancestors().map(|ancestor| ancestor.join(".git")));
            }
            protected = protected.into_iter().map(|path| normalize(&absolute(&path))).collect();
            protected.sort();
            protected.dedup();
            worktrees.push(WorktreeEventPolicy {
                key: key.clone(), root: normalize(&absolute(path)), plan: plan.clone(), protected,
            });
        }
        self.replace_policy(EventPolicy { worktrees });
    }

    pub(super) fn publish_plan_policy(&self) {
        let current = self.event_policy.read().map(|policy| policy.as_ref().clone()).unwrap_or_default();
        let worktrees = current.worktrees.into_iter().filter_map(|mut worktree| {
            let plan = self.plans.get(&worktree.key)?.clone();
            worktree.plan = plan;
            Some(worktree)
        }).collect();
        self.replace_policy(EventPolicy { worktrees });
    }

    pub(super) fn replace_policy(&self, policy: EventPolicy) {
        if let Ok(mut current) = self.event_policy.write() {
            *current = Arc::new(policy);
        }
    }

    pub(super) fn filter_ignored_batch(&self, batch: &mut WatchBatch) {
        let keys: Vec<_> = batch.worktrees.iter().cloned().collect();
        for key in keys {
            if batch.force_worktrees.contains(&key) {
                continue;
            }
            let allowed = batch.worktree_paths.get(&key).is_some_and(|paths| {
                paths.iter().any(|(path, kind)| !self.ignored_event(path, &key, kind))
            });
            if !allowed {
                batch.worktrees.remove(&key);
                batch.worktree_paths.remove(&key);
                batch.force_worktrees.remove(&key);
            }
        }
    }

    pub(super) fn plan_event(&self, path: &Path, key: &WorktreeKey, kind: &EventKind) -> bool {
        let tree = normalize(&key.path);
        if !path.starts_with(&tree) { return false; }
        if self.ignored_event(path, key, kind) { return false; }
        if path.file_name().is_some_and(|name| name == ".gitignore") { return true; }
        topology_event(kind) && (self.roots.contains_key(path) || path.is_dir()
            || matches!(kind, EventKind::Create(notify::event::CreateKind::Folder)
                | EventKind::Remove(notify::event::RemoveKind::Folder)))
    }

    pub(super) fn ignored_event(&self, path: &Path, key: &WorktreeKey, kind: &EventKind) -> bool {
        let Some(plan) = self.plans.get(key) else { return false; };
        if !plan.ignored_directories.iter().any(|directory| path == directory || path.starts_with(directory)) {
            return false;
        }
        if plan.tracked_files.contains(path) || topology_event(kind) && plan.tracked_directories.contains(path) {
            return false;
        }
        !rule_file(path)
    }
}
