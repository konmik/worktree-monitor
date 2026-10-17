use super::{absolute, add_target, forward_event, Registration, Target, WatchBoundary, WatchStatus};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const WATCH_RETRY: Duration = Duration::from_secs(5);

impl WatchBoundary {
    pub(super) fn request_rebuild(&mut self, retry_due: Option<Instant>) {
        self.force_rebuild = true;
        self.degraded = true;
        if let Some(retry_due) = retry_due { self.retry_due = Some(retry_due); }
    }

    pub(crate) fn next_retry(&self) -> Option<Instant> { self.retry_due }

    pub(super) fn create_watcher(&mut self, now: Instant) {
        let sender = self.sender.clone();
        let cancel = self.cancel.clone();
        let event_policy = self.event_policy.clone();
        let watcher = RecommendedWatcher::new(move |result| {
            let policy = event_policy.read().map(|policy| policy.clone()).unwrap_or_default();
            forward_event(result, &sender, &cancel, &policy);
        }, notify::Config::default());
        match watcher {
            Ok(watcher) => {
                self.watcher = Some(watcher);
                self.degraded = false;
                self.retry_due = None;
            }
            Err(_) => {
                self.watcher = None;
                self.degraded = true;
                self.retry_due = Some(now + WATCH_RETRY);
            }
        }
    }

    pub(super) fn rebuild_if_required(&mut self, now: Instant) -> bool {
        let full = self.force_rebuild || self.watcher.is_none();
        if self.force_rebuild {
            self.watcher = None;
            self.roots.clear();
            self.create_watcher(now);
        } else if self.watcher.is_none() {
            self.create_watcher(now);
        }
        full
    }

    pub(super) fn reconcile_status(&mut self, failed: bool, planning_failed: bool,
        desired: &HashMap<PathBuf, Registration>, now: Instant, mut full: bool) -> WatchStatus {
        if failed { full = true; }
        self.force_rebuild = failed;
        self.degraded = failed || self.watcher.is_none() || self.roots != *desired;
        if self.degraded || planning_failed {
            self.retry_due = Some(now + WATCH_RETRY);
        } else {
            self.retry_due = None;
            self.force_rebuild = false;
        }
        WatchStatus { full, degraded: self.degraded }
    }
}

pub(super) fn add_existing_or_parent(result: &mut HashMap<PathBuf, Registration>, path: &Path,
    mode: RecursiveMode, target: Target) {
    if path.is_dir() {
        add_target(result, path.to_path_buf(), mode, target);
    } else {
        add_recovery(result, path, target);
    }
}

pub(super) fn add_recovery(result: &mut HashMap<PathBuf, Registration>, path: &Path, target: Target) {
    add_recovery_with_mode(result, path, target, RecursiveMode::NonRecursive);
}

pub(super) fn add_recovery_with_mode(result: &mut HashMap<PathBuf, Registration>, path: &Path,
    target: Target, mode: RecursiveMode) {
    let parent = path.parent().unwrap_or(path);
    let root = nearest_existing(parent).unwrap_or_else(|| parent.to_path_buf());
    let mode = if root == parent { RecursiveMode::NonRecursive } else { mode };
    add_target(result, root, mode, target);
}

pub(super) fn recovery_root(path: &Path) -> PathBuf {
    let path = absolute(path);
    let parent = path.parent().unwrap_or(&path);
    nearest_existing(parent).unwrap_or_else(|| parent.to_path_buf())
}

fn nearest_existing(path: &Path) -> Option<PathBuf> {
    let mut path = absolute(path);
    loop {
        if path.exists() { return Some(path); }
        if !path.pop() { return None; }
    }
}
