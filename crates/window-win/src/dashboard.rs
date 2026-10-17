#[path = "features/window/repo-list/refresh-markers-identify-only-the-active-target/refresh.rs"]
mod refresh;
pub use refresh::RefreshTracker;
#[path = "features/window/idle-indicator/idle-percentage-shows-unused-monitor-time-over-the-last-minute/idle.rs"]
mod idle;

use std::path::{Path, PathBuf};
use crate::order::identity;
use crate::visibility::Visibility;
use wtm_core::{Error, Head, ScanTarget, Snapshot, WorktreeStats};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Health { Scanning, Live, Errors }

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ItemKey {
    Configuration,
    Launch,
    Presentation,
    Visibility,
    Empty,
    Repository(PathBuf),
    RepositoryError(PathBuf),
    Worktree(PathBuf, PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisplayItem {
    Status { health: Health, polling_time_last_minute: Duration },
    Repository { name: String, bare: bool },
    Worktree { path: PathBuf, result: Result<WorktreeStats, Error> },
    PendingWorktree { path: PathBuf },
    Notice { title: String, message: String, error: bool },
}

pub(crate) fn ellipsize_start<F>(value: &str, width: i32, measure: F) -> String
where F: Fn(&str) -> i32 {
    if width <= 0 { return String::new(); }
    if measure(value) <= width { return value.into(); }
    let ellipsis = "…";
    if measure(ellipsis) > width { return String::new(); }
    let mut result = ellipsis.to_owned();
    for (index, _) in value.char_indices().rev() {
        let mut candidate = String::with_capacity(ellipsis.len() + value.len() - index);
        candidate.push_str(ellipsis);
        candidate.push_str(&value[index..]);
        if measure(&candidate) <= width { result = candidate; }
    }
    result
}

fn head_caption(head: &Head) -> String {
    match head {
        Head::Branch(name) => name.clone(),
        _ => head.to_string(),
    }
}

impl DisplayItem {
    pub fn caption_with_refresh(&self, refreshing: bool) -> String {
        let mut caption = self.caption();
        if refreshing { caption.push_str("\nRefreshing"); }
        caption
    }

    pub fn caption(&self) -> String {
        match self {
            Self::Status { polling_time_last_minute, .. } => idle::caption(*polling_time_last_minute),
            Self::Repository { name, bare } => {
                let mut text = name.clone();
                if *bare { text.push_str("\nNo working trees (bare repository)."); }
                text
            }
            Self::Worktree { path, result } => match result {
                Ok(stats) => format!("{}\n{}", head_caption(&stats.head), stats.counts),
                Err(error) => format!("{}\nCounts unavailable · binaries unavailable\nERROR ({:?}): {}", path.display(), error.kind, error.message),
            },
            Self::PendingWorktree { path } => path.display().to_string(),
            Self::Notice { title, message, .. } => format!("{title}\n{message}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewItem {
    pub key: ItemKey,
    pub display: DisplayItem,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DashboardView {
    pub health: Health,
    pub items: Vec<ViewItem>,
}

fn name(path: &Path) -> String {
    path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned()
}

impl DashboardView {
    pub fn scanning() -> Self {
        Self { health: Health::Scanning, items: Vec::new() }
    }

    pub fn from_snapshot(snapshot: &Snapshot) -> Self {
        Self::from_snapshot_with_visibility(snapshot, &Visibility::default(), false)
    }

    pub fn from_snapshot_with_visibility(snapshot: &Snapshot, visibility: &Visibility, show_all: bool) -> Self {
        let mut items = Vec::new();
        if let Some(error) = &snapshot.config_error {
            items.push(ViewItem { key: ItemKey::Configuration, display: DisplayItem::Notice {
                title: "Configuration error".into(), message: format!("{:?}: {}", error.kind, error.message), error: true } });
        }
        if snapshot.repos.is_empty() {
            items.push(ViewItem { key: ItemKey::Empty, display: DisplayItem::Notice {
                title: "No registered repositories.".into(), message: String::new(), error: false } });
        }
        for repo in &snapshot.repos {
            items.push(ViewItem { key: ItemKey::Repository(identity(&repo.common_dir)),
                display: DisplayItem::Repository { name: name(&repo.path),
                    bare: repo.worktrees.as_ref().is_ok_and(|trees| trees.is_empty()) } });
            match &repo.worktrees {
                Ok(trees) => {
                    for tree in trees {
                        if !show_all && visibility.contains(&repo.common_dir, &tree.path) { continue; }
                        items.push(ViewItem { key: ItemKey::Worktree(identity(&repo.common_dir), identity(&tree.path)),
                            display: DisplayItem::Worktree { path: tree.path.clone(), result: tree.result.clone() } });
                    }
                }
                Err(error) => {
                    items.push(ViewItem { key: ItemKey::RepositoryError(identity(&repo.common_dir)),
                        display: DisplayItem::Notice { title: "Repository error · counts unavailable".into(),
                            message: format!("{}\n{:?}: {}", repo.path.display(), error.kind, error.message), error: true } });
                }
            }
        }
        Self { health: if snapshot.has_errors() { Health::Errors } else { Health::Live }, items }
    }

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

    pub fn launch_error(&mut self, error: &Error) {
        self.health = Health::Errors;
        self.items.insert(0, ViewItem { key: ItemKey::Launch, display: DisplayItem::Notice {
            title: "Launch repository discovery error".into(), message: format!("{:?}: {}", error.kind, error.message), error: true } });
    }

    pub fn presentation_warning(&mut self, message: &str) {
        self.health = Health::Errors;
        self.items.insert(0, ViewItem { key: ItemKey::Presentation, display: DisplayItem::Notice {
            title: "Presentation order warning".into(), message: message.into(), error: true } });
    }

    pub fn visibility_warning(&mut self, message: &str) {
        self.health = Health::Errors;
        self.items.insert(0, ViewItem { key: ItemKey::Visibility, display: DisplayItem::Notice {
            title: "Worktree visibility warning".into(), message: message.into(), error: true } });
    }
}

#[cfg(test)]
#[path = "dashboard.test.rs"]
mod tests;
