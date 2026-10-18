use super::Coordinator;
use crate::{Error, Snapshot};
use super::activity::{ScanActivity, ScanTarget};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct MonitorUpdate {
    pub snapshot: Arc<Snapshot>,
    pub scan_targets: Vec<ScanTarget>,
    pub scan_activities: Vec<ScanActivity>,
    pub launch_error: Option<Error>,
    pub polling_time_last_minute: Duration,
}

impl Coordinator {
    pub(super) fn publish(&mut self) {
        let repos = self.repo_order.iter().filter_map(|common| {
            let repo = &self.repos[common];
            match repo.discovered.as_ref()? {
                Err(error) => Some(crate::RepoEntry {
                    common_dir: repo.common_dir.clone(), path: repo.path.clone(), worktrees: Err(error.clone()),
                }),
                Ok(_) => {
                    let trees: Vec<_> = repo.worktrees.iter().filter_map(|key| {
                        let state = &self.worktrees[key];
                        state.last.as_ref().map(|last| crate::WorktreeEntry { path: state.path.clone(), result: last.clone() })
                    }).collect();
                    if trees.is_empty() && !repo.worktrees.is_empty() { return None; }
                    Some(crate::RepoEntry { common_dir: repo.common_dir.clone(), path: repo.path.clone(), worktrees: Ok(trees) })
                }
            }
        }).collect();
        let snapshot = Snapshot { repos, config_error: self.registrations.error.clone() };
        if snapshot != *self.snapshot { self.snapshot = Arc::new(snapshot); }
        self.publish_update();
    }

    pub(super) fn publish_update(&mut self) {
        let (scan_targets, scan_activities) = self.scan_activity();
        let update = MonitorUpdate {
            snapshot: self.snapshot.clone(), launch_error: self.launch_error.clone(),
            scan_targets, scan_activities,
            polling_time_last_minute: self.polling_time.total(Instant::now()),
        };
        let update = Arc::new(update);
        #[cfg(test)]
        self.published.push(update.clone());
        *self.latest.lock().unwrap_or_else(|error| error.into_inner()) = Some(update);
    }
}
