use super::{Coordinator, WorktreeKey};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ScanTarget {
    Repository { common_dir: PathBuf, path: PathBuf },
    Worktree { repository: PathBuf, path: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanActivity {
    pub target: ScanTarget,
    pub generation: u64,
    pub started_at: Instant,
    pub finished_at: Option<Instant>,
}

impl Coordinator {
    pub(super) fn start_scan(&mut self, common: &Path, tree: Option<&WorktreeKey>) {
        self.scan_generation += 1;
        let repo = self.repos.get_mut(common).unwrap();
        let started_at = Instant::now();
        if let Some(key) = tree {
            let state = self.worktrees.get_mut(key).unwrap();
            state.activity = Some(ScanActivity {
                target: ScanTarget::Worktree { repository: repo.common_dir.clone(), path: state.path.clone() },
                generation: self.scan_generation, started_at, finished_at: None,
            });
        } else {
            repo.activity = Some(ScanActivity {
                target: ScanTarget::Repository { common_dir: repo.common_dir.clone(), path: repo.path.clone() },
                generation: self.scan_generation, started_at, finished_at: None,
            });
        }
        self.publish_update();
    }

    pub(super) fn finish_scan(&mut self) {
        let now = Instant::now();
        for activity in self.repos.values_mut().filter_map(|repo| repo.activity.as_mut())
            .chain(self.worktrees.values_mut().filter_map(|tree| tree.activity.as_mut())) {
            if activity.finished_at.is_none() { activity.finished_at = Some(now); }
        }
    }

    pub(super) fn scan_activity(&self) -> (Vec<ScanTarget>, Vec<ScanActivity>) {
        let mut scan_targets = Vec::new();
        let mut scan_activities = Vec::new();
        for common in &self.repo_order {
            let repo = &self.repos[common];
            let target = ScanTarget::Repository { common_dir: repo.common_dir.clone(), path: repo.path.clone() };
            if let Some(activity) = &repo.activity {
                scan_activities.push(ScanActivity { target: target.clone(), ..activity.clone() });
            }
            scan_targets.push(target);
            for key in &repo.worktrees {
                let state = &self.worktrees[key];
                let target = ScanTarget::Worktree { repository: repo.common_dir.clone(), path: state.path.clone() };
                if let Some(activity) = &state.activity {
                    scan_activities.push(ScanActivity { target: target.clone(), ..activity.clone() });
                }
                scan_targets.push(target);
            }
        }
        (scan_targets, scan_activities)
    }
}
