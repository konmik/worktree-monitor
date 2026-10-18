use crate::dashboard::{DashboardView, DisplayItem, Health, ItemKey, ViewItem, name};
use crate::order::identity;
use crate::visibility::Visibility;
use wtm_core::{ScanTarget, Snapshot};

impl DashboardView {
    pub fn from_scan(snapshot: &Snapshot, targets: &[ScanTarget], visibility: &Visibility, show_all: bool) -> Self {
        let mut items = Vec::new();
        let mut errors = snapshot.config_error.is_some();
        let mut pending = false;
        if let Some(error) = &snapshot.config_error {
            items.push(ViewItem { key: ItemKey::Configuration, display: DisplayItem::Notice {
                title: "Configuration error".into(), message: format!("{:?}: {}", error.kind, error.message), error: true } });
        }
        for target in targets {
            let ScanTarget::Repository { common_dir, path } = target else { continue; };
            let repository = identity(common_dir);
            let completed = snapshot.repos.iter().find(|repo| identity(&repo.common_dir) == repository);
            let children: Vec<_> = targets.iter().filter_map(|target| match target {
                ScanTarget::Worktree { repository: common, path } if identity(common) == repository => Some(path),
                _ => None,
            }).collect();
            pending |= completed.is_none() && children.is_empty();
            items.push(ViewItem { key: ItemKey::Repository(repository.clone()), display: DisplayItem::Repository {
                name: name(completed.map_or(path, |repo| &repo.path)),
                bare: completed.is_some_and(|repo| repo.worktrees.as_ref().is_ok_and(|trees| trees.is_empty())) && children.is_empty(),
            } });
            if let Some(repo) = completed.filter(|repo| repo.worktrees.is_err()) {
                let error = repo.worktrees.as_ref().unwrap_err();
                errors = true;
                items.push(ViewItem { key: ItemKey::RepositoryError(repository), display: DisplayItem::Notice {
                    title: "Repository error · counts unavailable".into(),
                    message: format!("{}\n{:?}: {}", repo.path.display(), error.kind, error.message), error: true } });
                continue;
            }
            for path in children {
                let tree = completed.and_then(|repo| repo.worktrees.as_ref().ok())
                    .and_then(|trees| trees.iter().find(|tree| identity(&tree.path) == identity(path)));
                pending |= tree.is_none();
                errors |= tree.is_some_and(|tree| tree.result.is_err());
                if !show_all && visibility.contains(&repository, path) { continue; }
                items.push(ViewItem { key: ItemKey::Worktree(repository.clone(), identity(path)), display: match tree {
                    Some(tree) => DisplayItem::Worktree { path: tree.path.clone(), result: tree.result.clone() },
                    None => DisplayItem::PendingWorktree { path: path.clone() },
                } });
            }
        }
        if !items.iter().any(|item| matches!(item.key, ItemKey::Repository(_))) {
            items.push(ViewItem { key: ItemKey::Empty, display: DisplayItem::Notice {
                title: "No registered repositories.".into(), message: String::new(), error: false } });
        }
        Self { health: if errors { Health::Errors } else if pending { Health::Scanning } else { Health::Live }, items }
    }
}
