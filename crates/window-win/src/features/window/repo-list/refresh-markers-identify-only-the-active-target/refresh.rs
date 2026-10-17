use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use crate::dashboard::ItemKey;
use crate::order::identity;
use wtm_core::{ScanActivity, ScanTarget};

#[derive(Default)]
pub struct RefreshTracker {
    entries: HashMap<ItemKey, (u64, Instant)>,
}

impl RefreshTracker {
    pub fn update(&mut self, activities: &[ScanActivity], now: Instant) -> HashSet<ItemKey> {
        let mut present = HashSet::new();
        let mut refreshing = HashSet::new();
        for activity in activities {
            let key = match &activity.target {
                ScanTarget::Repository { common_dir, .. } => ItemKey::Repository(identity(common_dir)),
                ScanTarget::Worktree { repository, path } => ItemKey::Worktree(identity(repository), identity(path)),
            };
            present.insert(key.clone());
            let entry = self.entries.entry(key.clone()).or_insert_with(|| (activity.generation, now + Duration::from_secs(1)));
            if entry.0 != activity.generation {
                *entry = (activity.generation, now + Duration::from_secs(1));
            }
            if activity.finished_at.is_none() || now < entry.1 {
                refreshing.insert(key);
            }
        }
        self.entries.retain(|key, _| present.contains(key));
        refreshing
    }
}

