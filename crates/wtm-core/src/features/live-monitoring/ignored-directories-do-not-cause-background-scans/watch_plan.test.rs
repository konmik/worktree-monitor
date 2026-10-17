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
