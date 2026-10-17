use super::{absolute, normalize, recovery_root};
use crate::git;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorktreeWatchPlan {
    pub(super) recursive_roots: Vec<PathBuf>,
    pub(super) directory_gates: Vec<PathBuf>,
    pub(super) ignored_directories: HashSet<PathBuf>,
    pub(super) tracked_files: HashSet<PathBuf>,
    pub(super) tracked_directories: HashSet<PathBuf>,
}

pub(super) fn plan_for(tree: &Path, paths: &git::WatchPaths) -> WorktreeWatchPlan {
    let mut children: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for directory in &paths.directories {
        if directory.as_os_str().is_empty() { continue; }
        let parent = directory.parent().unwrap_or_else(|| Path::new("")).to_path_buf();
        children.entry(parent).or_default().push(directory.clone());
    }
    for entries in children.values_mut() { entries.sort(); }
    let mut tracked_directories = HashSet::new();
    for file in &paths.tracked {
        let mut directory = file.parent();
        while let Some(current) = directory.filter(|current| !current.as_os_str().is_empty()) {
            tracked_directories.insert(current.to_path_buf());
            directory = current.parent();
        }
    }
    let tree = absolute(tree);
    let ignored = &paths.ignored_directories;
    let ignored_directories: HashSet<_> = ignored.iter().map(|directory| normalize(&tree.join(directory))).collect();
    let tracked_files = paths.tracked.iter().map(|file| normalize(&tree.join(file)))
        .filter(|file| file.ancestors().any(|ancestor| ignored_directories.contains(ancestor))).collect();
    let tracked_directories_absolute = tracked_directories.iter()
        .map(|directory| normalize(&tree.join(directory))).collect();
    let mut plan = WorktreeWatchPlan {
        recursive_roots: Vec::new(), directory_gates: Vec::new(), ignored_directories,
        tracked_files, tracked_directories: tracked_directories_absolute,
    };
    plan.directory_gates.push(normalize(&tree));
    visit_permitted(Path::new(""), &children, ignored, &tracked_directories, &tree, &mut plan);
    plan.recursive_roots.sort();
    plan.recursive_roots.dedup();
    plan.directory_gates.sort();
    plan.directory_gates.dedup();
    plan
}

fn has_ignored(relative: &Path, children: &HashMap<PathBuf, Vec<PathBuf>>,
    ignored: &HashSet<PathBuf>) -> bool {
    children.get(relative).into_iter().flatten().any(|child| {
        ignored.contains(child) || has_ignored(child, children, ignored)
    })
}

fn visit_permitted(relative: &Path, children: &HashMap<PathBuf, Vec<PathBuf>>,
    ignored: &HashSet<PathBuf>, tracked_directories: &HashSet<PathBuf>, tree: &Path,
    plan: &mut WorktreeWatchPlan) {
    for child in children.get(relative).into_iter().flatten() {
        if ignored.contains(child) {
            if tracked_directories.contains(child) {
                add_tracked_gates(child, tracked_directories, tree, plan);
            }
        } else if has_ignored(child, children, ignored) {
            plan.directory_gates.push(normalize(&tree.join(child)));
            visit_permitted(child, children, ignored, tracked_directories, tree, plan);
        } else {
            plan.recursive_roots.push(normalize(&tree.join(child)));
        }
    }
}

fn add_tracked_gates(relative: &Path, tracked_directories: &HashSet<PathBuf>,
    tree: &Path, plan: &mut WorktreeWatchPlan) {
    let mut gates: Vec<_> = tracked_directories.iter()
        .filter(|directory| **directory == relative || directory.starts_with(relative))
        .collect();
    gates.sort_by_key(|directory| directory.components().count());
    for directory in gates {
        plan.directory_gates.push(normalize(&tree.join(directory)));
    }
}

pub(super) fn fallback_plan(path: &Path) -> WorktreeWatchPlan {
    let path = absolute(path);
    let root = if path.is_dir() { path } else { recovery_root(&path) };
    WorktreeWatchPlan {
        recursive_roots: vec![normalize(&root)], directory_gates: Vec::new(),
        ignored_directories: HashSet::new(), tracked_files: HashSet::new(), tracked_directories: HashSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracked_file_cache_keeps_only_ignored_exceptions() {
        let root = absolute(Path::new("watch-root"));
        let paths = git::WatchPaths {
            directories: ["visible", "ignored", "ignored/nested", "ignored-sibling", "ignored-module"]
                .into_iter().map(PathBuf::from).collect(),
            ignored_directories: ["ignored", "ignored-module"].into_iter().map(PathBuf::from).collect(),
            tracked: ["visible/tracked.txt", "ignored/nested/tracked.txt", "ignored-module", "ignored-sibling/tracked.txt"]
                .into_iter().map(PathBuf::from).collect(),
        };
        let plan = plan_for(&root, &paths);
        assert_eq!(plan.tracked_files, ["ignored/nested/tracked.txt", "ignored-module"]
            .into_iter().map(|path| normalize(&root.join(path))).collect());
        assert_eq!(plan.tracked_directories, ["visible", "ignored", "ignored/nested", "ignored-sibling"]
            .into_iter().map(|path| normalize(&root.join(path))).collect());
    }

    #[test]
    fn tracked_file_cache_is_empty_without_ignored_directories() {
        let root = absolute(Path::new("watch-root"));
        let plan = plan_for(&root, &git::WatchPaths {
            directories: vec![PathBuf::from("visible")],
            ignored_directories: HashSet::new(),
            tracked: vec![PathBuf::from("visible/tracked.txt")],
        });
        assert!(plan.tracked_files.is_empty());
        assert_eq!(plan.tracked_directories, HashSet::from([normalize(&root.join("visible"))]));
    }

    #[cfg(windows)]
    #[test]
    fn tracked_file_cache_normalizes_before_matching_ignored_directories() {
        let root = absolute(Path::new("Watch-Root"));
        let plan = plan_for(&root, &git::WatchPaths {
            directories: vec![PathBuf::from("Ignored")],
            ignored_directories: HashSet::from([PathBuf::from("Ignored")]),
            tracked: vec![PathBuf::from("IGNORED/Tracked.txt")],
        });
        assert_eq!(plan.tracked_files, HashSet::from([normalize(&root.join("ignored/tracked.txt"))]));
    }
}
