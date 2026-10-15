use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use crate::order::identity;
use crate::visibility::Visibility;
use wts_core::{Error, Head, ScanActivity, ScanTarget, Snapshot, WorktreeStats};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Health { Scanning, Live, Errors }

impl Health {
    pub fn caption(self, polling_time_last_minute: Duration) -> String {
        let label = match self { Self::Scanning => "SCANNING", Self::Live => "LIVE", Self::Errors => "ERROR" };
        format!("{label} · Polling {} in last minute", duration_caption(polling_time_last_minute))
    }
}

fn duration_caption(duration: Duration) -> String {
    let tenths = duration.as_millis().saturating_add(50) / 100;
    format!("{}.{:01}s", tenths / 10, tenths % 10)
}

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisplayItem {
    Status { health: Health, polling_time_last_minute: Duration },
    Repository { name: String, worktrees: Option<usize>, bare: bool },
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
            Self::Status { health, polling_time_last_minute } => health.caption(*polling_time_last_minute),
            Self::Repository { name, worktrees, bare } => {
                let mut text = format!("{name} [{}]", worktrees.map_or_else(|| "?".into(), |count| count.to_string()));
                if *bare { text.push_str("\nNo working trees (bare repository)."); }
                text
            }
            Self::Worktree { path, result } => match result {
                Ok(stats) => format!("{}\n{}", head_caption(&stats.head), stats.counts),
                Err(error) => format!("{}\nCounts unavailable · binaries unavailable\nERROR ({:?}): {}", path.display(), error.kind, error.message),
            },
            Self::PendingWorktree { path } => format!("{}\nBranch pending · counts pending · binaries pending", path.display()),
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
        let mut view = Self::from_snapshot_with_visibility(snapshot, visibility, show_all);
        for target in targets {
            let ScanTarget::Repository { common_dir, path } = target else { continue; };
            let key = ItemKey::Repository(identity(common_dir));
            if !view.items.iter().any(|item| item.key == key) {
                view.items.push(ViewItem { key, display: DisplayItem::Repository {
                    name: name(path), worktrees: None, bare: false,
                } });
            }
        }
        for target in targets {
            let ScanTarget::Worktree { repository, path } = target else { continue; };
            let repository = identity(repository);
            let key = ItemKey::Worktree(repository.clone(), identity(path));
            if view.items.iter().any(|item| item.key == key)
                || (!show_all && visibility.contains(&repository, path))
                || snapshot.repos.iter().any(|repo| identity(&repo.common_dir) == repository && repo.worktrees.is_err()) {
                continue;
            }
            let Some(header) = view.items.iter().position(|item| item.key == ItemKey::Repository(repository.clone())) else { continue; };
            let end = view.items.iter().enumerate().skip(header + 1)
                .find(|(_, item)| matches!(item.key, ItemKey::Repository(_))).map_or(view.items.len(), |(index, _)| index);
            view.items.insert(end, ViewItem { key, display: DisplayItem::PendingWorktree { path: path.clone() } });
        }
        for item in &mut view.items {
            let ItemKey::Repository(repository) = &item.key else { continue; };
            if snapshot.repos.iter().any(|repo| identity(&repo.common_dir) == *repository && repo.worktrees.is_err()) { continue; }
            let discovered = targets.iter().filter(|target| matches!(target,
                ScanTarget::Worktree { repository: common, .. } if identity(common) == *repository)).count();
            if discovered > 0 {
                if let DisplayItem::Repository { worktrees, bare, .. } = &mut item.display {
                    *worktrees = Some(discovered);
                    *bare = false;
                }
            }
        }
        if view.items.iter().any(|item| matches!(item.key, ItemKey::Repository(_))) {
            view.items.retain(|item| item.key != ItemKey::Empty);
        }
        if !snapshot.has_errors() && view.items.iter().any(|item| matches!(item.display,
            DisplayItem::PendingWorktree { .. } | DisplayItem::Repository { worktrees: None, .. })) {
            view.health = Health::Scanning;
        }
        view
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
