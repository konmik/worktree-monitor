use std::path::{Path, PathBuf};
use wts_core::{Error, Snapshot, WorktreeStats};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Total {
    pub value: Option<u128>,
    pub partial: bool,
}

impl Total {
    fn known(value: u128) -> Self { Self { value: Some(value), partial: false } }

    fn add(&mut self, value: u64) {
        self.value = self.value.and_then(|sum| sum.checked_add(u128::from(value)));
    }

    pub fn text(self) -> String {
        self.value.map_or_else(|| "?".into(), |value| value.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Totals {
    pub repositories: Total,
    pub worktrees: Total,
    pub additions: Total,
    pub deletions: Total,
    pub binaries: Total,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Health { Scanning, Live, Errors }

impl Health {
    pub fn caption(self, interval: u64) -> String {
        let label = match self { Self::Scanning => "SCANNING", Self::Live => "LIVE", Self::Errors => "ERROR" };
        format!("{label} · Polling interval {interval}s")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Metric { Repositories, Worktrees, Changes }

impl Metric {
    pub fn label(self) -> &'static str {
        match self { Self::Repositories => "REPOSITORIES", Self::Worktrees => "WORKTREES", Self::Changes => "LINE CHANGES" }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ItemKey {
    Configuration,
    Empty,
    Repository(PathBuf, PathBuf),
    Worktree(PathBuf, PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisplayItem {
    Brand,
    Status { health: Health, interval: u64 },
    Metric { kind: Metric, primary: Total, secondary: Option<Total> },
    Repository { name: String, worktrees: Total, bare: bool },
    Worktree { name: String, path: PathBuf, result: Result<WorktreeStats, Error> },
    Notice { title: String, message: String, error: bool },
}

impl DisplayItem {
    pub fn caption(&self) -> String {
        match self {
            Self::Brand => "WTS\nWORKTREE MONITOR".into(),
            Self::Status { health, interval } => health.caption(*interval),
            Self::Metric { kind, primary, secondary } => {
                let value = if let Some(second) = secondary {
                    format!("+{} -{}", primary.text(), second.text())
                } else { primary.text() };
                let detail = if primary.value.is_none() { "Awaiting scan" }
                    else if primary.partial { "Known · partial" }
                    else { "Registered" };
                let detail = if *kind == Metric::Worktrees && !primary.partial && primary.value.is_some() { "Discovered" }
                    else if *kind == Metric::Changes && !primary.partial && primary.value.is_some() { "Measured lines" }
                    else { detail };
                format!("{}\n{value}\n{detail}", kind.label())
            }
            Self::Repository { name, worktrees, bare } => {
                let label = if worktrees.value == Some(1) { "worktree" } else { "worktrees" };
                let mut text = format!("{name} · {} {label}", worktrees.text());
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
    pub totals: Totals,
    pub health: Health,
    pub items: Vec<ViewItem>,
}

fn name(path: &Path) -> String {
    path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned()
}

impl DashboardView {
    pub fn scanning() -> Self {
        let unknown = Total { value: None, partial: false };
        Self { totals: Totals { repositories: unknown, worktrees: unknown, additions: unknown, deletions: unknown, binaries: unknown },
            health: Health::Scanning, items: vec![ViewItem { key: ItemKey::Empty,
                display: DisplayItem::Notice { title: "Scanning repositories…".into(), message: "Waiting for the first snapshot".into(), error: false } }] }
    }

    pub fn from_snapshot(snapshot: &Snapshot) -> Self {
        let zero = Total::known(0);
        let mut totals = Totals { repositories: Total::known(snapshot.repos.len() as u128), worktrees: zero,
            additions: zero, deletions: zero, binaries: zero };
        let mut items = Vec::new();
        if let Some(error) = &snapshot.config_error {
            totals.repositories.partial = true;
            totals.worktrees.partial = true;
            totals.additions.partial = true;
            totals.deletions.partial = true;
            totals.binaries.partial = true;
            let retained = if snapshot.repos.is_empty() { "" } else { "\nUsing last good registrations." };
            items.push(ViewItem { key: ItemKey::Configuration, display: DisplayItem::Notice {
                title: "Configuration error".into(), message: format!("{:?}: {}{retained}", error.kind, error.message), error: true } });
        }
        if snapshot.repos.is_empty() {
            items.push(ViewItem { key: ItemKey::Empty, display: DisplayItem::Notice {
                title: "No registered repositories.".into(), message: String::new(), error: false } });
        }
        for repo in &snapshot.repos {
            let count = match &repo.worktrees {
                Ok(trees) => Total::known(trees.len() as u128),
                Err(_) => Total { value: None, partial: true },
            };
            items.push(ViewItem { key: ItemKey::Repository(repo.common_dir.clone(), repo.path.clone()),
                display: DisplayItem::Repository { name: name(&repo.path), worktrees: count,
                    bare: repo.worktrees.as_ref().is_ok_and(|trees| trees.is_empty()) } });
            match &repo.worktrees {
                Ok(trees) => {
                    totals.worktrees.value = totals.worktrees.value.and_then(|sum| sum.checked_add(trees.len() as u128));
                    for tree in trees {
                        match &tree.result {
                            Ok(stats) => {
                                totals.additions.add(stats.counts.additions);
                                totals.deletions.add(stats.counts.deletions);
                                totals.binaries.add(stats.counts.binaries);
                            }
                            Err(_) => {
                                totals.additions.partial = true;
                                totals.deletions.partial = true;
                                totals.binaries.partial = true;
                            }
                        }
                        items.push(ViewItem { key: ItemKey::Worktree(repo.common_dir.clone(), tree.path.clone()),
                            display: DisplayItem::Worktree { name: name(&tree.path), path: tree.path.clone(), result: tree.result.clone() } });
                    }
                }
                Err(error) => {
                    totals.worktrees.partial = true;
                    totals.additions.partial = true;
                    totals.deletions.partial = true;
                    totals.binaries.partial = true;
                    items.push(ViewItem { key: ItemKey::Worktree(repo.common_dir.clone(), repo.path.clone()),
                        display: DisplayItem::Notice { title: "Repository error · counts unavailable".into(),
                            message: format!("{}\n{:?}: {}", repo.path.display(), error.kind, error.message), error: true } });
                }
            }
        }
        Self { totals, health: if snapshot.has_errors() { Health::Errors } else { Health::Live }, items }
    }

    pub fn metrics(&self) -> [DisplayItem; 3] {
        [DisplayItem::Metric { kind: Metric::Repositories, primary: self.totals.repositories, secondary: None },
            DisplayItem::Metric { kind: Metric::Worktrees, primary: self.totals.worktrees, secondary: None },
            DisplayItem::Metric { kind: Metric::Changes, primary: self.totals.additions, secondary: Some(self.totals.deletions) }]
    }
}
