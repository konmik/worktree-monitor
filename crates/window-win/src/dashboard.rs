use std::path::{Path, PathBuf};
use crate::order::identity;
use wts_core::{Error, Head, Snapshot, WorktreeStats};

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
    Worktree { path: PathBuf, result: Result<WorktreeStats, Error> },
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
            Self::Worktree { path, result } => match result {
                Ok(stats) => {
                    let counts = stats.counts;
                    let binaries = if counts.binaries > 0 { format!(" · binaries: {}", counts.binaries) } else { String::new() };
                    format!("{}\n{}\n+{} -{}{}", head_caption(&stats.head), path.display(),
                        counts.additions, counts.deletions, binaries)
                }
                Err(error) => format!("{}\nCounts unavailable · binaries unavailable\nERROR ({:?}): {}", path.display(), error.kind, error.message),
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

    pub fn presentation_warning(&mut self, message: &str) {
        self.health = Health::Errors;
        self.items.insert(0, ViewItem { key: ItemKey::Presentation, display: DisplayItem::Notice {
            title: "Presentation order warning".into(), message: message.into(), error: true } });
    }
}

#[cfg(test)]
mod tests {
    use super::ellipsize_start;

    #[test]
    fn ellipsize_start_keeps_the_longest_measured_suffix() {
        let measure = |value: &str| value.chars().map(|character| if character == '界' { 2 } else { 1 }).sum();
        assert_eq!(ellipsize_start("ab界cd", 5, measure), "…界cd");
        assert_eq!(ellipsize_start("already short", 20, |value| value.chars().count() as i32), "already short");
    }

    #[test]
    fn ellipsize_start_respects_unicode_boundaries_and_short_widths() {
        let measure = |value: &str| value.chars().count() as i32;
        assert_eq!(ellipsize_start("a🌲cde", 4, measure), "…cde");
        assert_eq!(ellipsize_start("abcdef", 1, measure), "…");
        assert_eq!(ellipsize_start("abcdef", 0, measure), "");
    }
}
