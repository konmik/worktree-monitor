use crate::dashboard::{DashboardView, ItemKey};
use crate::visibility::Visibility;
use crate::Snapshot;

impl DashboardView {
    pub fn from_snapshot_with_visibility(snapshot: &Snapshot, visibility: &Visibility, show_all: bool) -> Self {
        let mut view = Self::from_snapshot(snapshot);
        if !show_all {
            view.items.retain(|item| match &item.key {
                ItemKey::Worktree(repository, worktree) => !visibility.contains(repository, worktree),
                _ => true,
            });
        }
        view
    }
}
