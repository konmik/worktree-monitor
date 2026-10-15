use std::path::{Path, PathBuf};
use crate::order::identity;
use wts_core::{Error, Snapshot, WorktreeStats};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Health { Scanning, Live, Errors }

impl Health {
    pub fn caption(self, interval: u64) -> String {
        let label = match self { Self::Scanning => "SCANNING", Self::Live => "LIVE", Self::Errors => "ERROR" };
        format!("{label} · Polling interval {interval}s")
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ItemKey {
    Configuration,
    Launch,
    Presentation,
    Empty,
    Repository(PathBuf),
    RepositoryError(PathBuf),
    Worktree(PathBuf, PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisplayItem {
    Brand,
    Status { health: Health, interval: u64 },
    Repository { name: String, worktrees: Option<usize>, bare: bool },
    Worktree { name: String, path: PathBuf, result: Result<WorktreeStats, Error> },
    Notice { title: String, message: String, error: bool },
}

impl DisplayItem {
    pub fn caption(&self) -> String {
        match self {
            Self::Brand => "WTS\nWORKTREE MONITOR".into(),
            Self::Status { health, interval } => health.caption(*interval),
            Self::Repository { name, worktrees, bare } => {
                let label = if *worktrees == Some(1) { "worktree" } else { "worktrees" };
                let mut text = format!("{name} · {} {label}", worktrees.map_or_else(|| "?".into(), |count| count.to_string()));
                if *bare { text.push_str("\nNo working trees (bare repository)."); }
                text
            }
            Self::Worktree { name, path, result } => match result {
                Ok(stats) => {
                    let counts = stats.counts;
                    let mut text = format!("{}\n{name}\n{}\n+{} -{} · binaries: {}", stats.head, path.display(),
                        counts.additions, counts.deletions, counts.binaries);
                    if counts.additions == 0 && counts.deletions == 0 {
                        text.push_str(if counts.binaries == 0 { "\nZero measured changes" } else { "\nBinary changes · No measured line changes" });
                    } else { text.push_str("\nMeasured line changes"); }
                    text
                }
                Err(error) => format!("{name}\n{}\nCounts unavailable · binaries unavailable\nERROR ({:?}): {}", path.display(), error.kind, error.message),
            },
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
        Self { health: Health::Scanning, items: vec![ViewItem { key: ItemKey::Empty,
                display: DisplayItem::Notice { title: "Scanning repositories…".into(), message: "Waiting for the first snapshot".into(), error: false } }] }
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
            let count = match &repo.worktrees {
                Ok(trees) => Some(trees.len()),
                Err(_) => None,
            };
            items.push(ViewItem { key: ItemKey::Repository(identity(&repo.common_dir)),
                display: DisplayItem::Repository { name: name(&repo.path), worktrees: count,
                    bare: repo.worktrees.as_ref().is_ok_and(|trees| trees.is_empty()) } });
            match &repo.worktrees {
                Ok(trees) => {
                    for tree in trees {
                        items.push(ViewItem { key: ItemKey::Worktree(identity(&repo.common_dir), identity(&tree.path)),
                            display: DisplayItem::Worktree { name: name(&tree.path), path: tree.path.clone(), result: tree.result.clone() } });
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

    pub fn presentation_warning(&mut self, message: &str) {
        self.health = Health::Errors;
        self.items.insert(0, ViewItem { key: ItemKey::Presentation, display: DisplayItem::Notice {
            title: "Presentation order warning".into(), message: message.into(), error: true } });
    }
}
