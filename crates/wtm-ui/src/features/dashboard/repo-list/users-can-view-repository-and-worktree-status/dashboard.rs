#[path = "../refresh-markers-identify-only-the-active-target/refresh.rs"]
mod refresh;
pub use refresh::RefreshTracker;
#[path = "../../idle-indicator/idle-percentage-shows-unused-monitor-time-over-the-last-minute/idle.rs"]
mod idle;

use std::path::{Path, PathBuf};
use crate::order::identity;
use crate::{Error, Head, Snapshot, WorktreeStats};
use crate::IdlePercentage;

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
    Status { health: Health, idle: IdlePercentage },
    Repository { name: String, bare: bool },
    Worktree { path: PathBuf, result: Result<WorktreeStats, Error> },
    PendingWorktree { path: PathBuf },
    Notice { title: String, message: String, error: bool },
}


pub(crate) fn head_caption(head: &Head) -> String {
    match head {
        Head::Branch(name) => name.clone(),
        _ => head.to_string(),
    }
}

impl DisplayItem {


    pub fn caption(&self) -> String {
        match self {
            Self::Status { idle, .. } => idle::caption(*idle),
            Self::Repository { name, bare } => {
                let mut text = name.clone();
                if *bare { text.push_str("\nNo working trees (bare repository)."); }
                text
            }
            Self::Worktree { path, result } => match result {
                Ok(stats) => {
                    let indicator = crate::commit_indicators::indicator(stats.pending_commits)
                        .map(|text| format!(" {text}")).unwrap_or_default();
                    format!("{}{indicator}\n{}", head_caption(&stats.head), stats.counts)
                }
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

pub(crate) fn name(path: &Path) -> String {
    path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned()
}

impl DashboardView {
    pub fn scanning() -> Self {
        Self { health: Health::Scanning, items: Vec::new() }
    }



    pub fn from_snapshot(snapshot: &Snapshot) -> Self {
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


    pub fn launch_error(&mut self, error: &Error) {
        self.health = Health::Errors;
        self.items.insert(0, ViewItem { key: ItemKey::Launch, display: DisplayItem::Notice {
            title: "Launch repository discovery error".into(), message: format!("{:?}: {}", error.kind, error.message), error: true } });
    }




}
