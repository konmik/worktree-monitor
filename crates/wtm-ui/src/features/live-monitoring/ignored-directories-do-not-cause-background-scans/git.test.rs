use super::*;

#[test]
fn included_configuration_is_watched_before_it_sets_excludes() {
    let parent = std::env::temp_dir().join("opencode");
    std::fs::create_dir_all(&parent).unwrap();
    let directory = tempfile::Builder::new().prefix("wtm-ignore-config-").tempdir_in(parent).unwrap();
    let tree = directory.path();
    let cancel = Cancel::new();
    process::checked(Some(tree), &args(&["init", "--quiet"]), &cancel).unwrap();
    let included = tree.join(".git/included config");
    std::fs::write(&included, b"").unwrap();
    let mut command = args(&["config", "include.path"]);
    command.push("included config".into());
    process::checked(Some(tree), &command, &cancel).unwrap();
    let sources = monitoring_sources(tree, &cancel).unwrap();
    assert!(sources.repository.iter().any(|source| crate::poll::normalize(source) == crate::poll::normalize(&included)));
}

#[test]
fn watch_paths_checks_ignores_before_descending_and_lists_cached_paths_only() {
    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path().join("repo");
    std::fs::create_dir(&tree).unwrap();
    let cancel = Cancel::new();
    process::checked(Some(&tree), &args(&["init", "--quiet"]), &cancel).unwrap();
    std::fs::write(tree.join(".gitignore"), b"ignored/\n").unwrap();
    std::fs::create_dir_all(tree.join("ignored/noise/deep")).unwrap();
    std::fs::create_dir_all(tree.join("ignored/disposable/hidden/deep")).unwrap();
    std::fs::create_dir(tree.join("empty")).unwrap();
    std::fs::write(tree.join("ignored/noise/deep/tracked.txt"), b"tracked\n").unwrap();
    process::checked(Some(&tree), &args(&["add", ".gitignore"]), &cancel).unwrap();
    process::checked(Some(&tree), &args(&["add", "-f", "ignored/noise/deep/tracked.txt"]), &cancel).unwrap();
    std::fs::write(tree.join("untracked.txt"), b"untracked\n").unwrap();
    let paths = watch_paths(&tree, &cancel).unwrap();
    assert!(paths.directories.contains(&PathBuf::from("empty")));
    assert!(paths.directories.contains(&PathBuf::from("ignored")));
    assert!(paths.directories.contains(&PathBuf::from("ignored/noise")));
    assert!(paths.directories.contains(&PathBuf::from("ignored/noise/deep")));
    assert!(paths.ignored_directories.contains(&PathBuf::from("ignored")));
    assert!(paths.ignored_directories.contains(&PathBuf::from("ignored/noise")));
    assert!(paths.tracked.contains(&PathBuf::from(".gitignore")));
    assert!(paths.tracked.contains(&PathBuf::from("ignored/noise/deep/tracked.txt")));
    assert!(!paths.tracked.contains(&PathBuf::from("untracked.txt")));
    assert!(!paths.directories.contains(&PathBuf::from("ignored/disposable/hidden")));
}

#[test]
fn watch_paths_preserves_broad_directory_coverage_and_prunes_ignored_children() {
    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path();
    let cancel = Cancel::new();
    process::checked(Some(tree), &args(&["init", "--quiet"]), &cancel).unwrap();
    std::fs::write(tree.join(".gitignore"), b"ignored/\n").unwrap();
    for index in 0..48 {
        std::fs::create_dir_all(tree.join(format!("branch-{index}/visible/deep"))).unwrap();
        std::fs::create_dir_all(tree.join(format!("branch-{index}/ignored/hidden"))).unwrap();
    }
    let paths = watch_paths(tree, &cancel).unwrap();
    assert_eq!(paths.directories.len(), 1 + 48 * 4);
    assert_eq!(paths.ignored_directories.len(), 48);
    for index in 0..48 {
        assert!(paths.directories.contains(&PathBuf::from(format!("branch-{index}/visible/deep"))));
        assert!(paths.ignored_directories.contains(&PathBuf::from(format!("branch-{index}/ignored"))));
        assert!(!paths.directories.contains(&PathBuf::from(format!("branch-{index}/ignored/hidden"))));
    }
}

#[test]
fn cancelled_directory_walk_stops_before_reading_the_next_directory() {
    let directory = tempfile::tempdir().unwrap();
    let cancel = Cancel::new();
    cancel.cancel();
    let error = watch_directories(&directory.path().join("missing"), &[], &cancel).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Cancelled);
}

#[test]
fn watch_paths_preserves_negated_directory_listing() {
    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path().join("repo");
    std::fs::create_dir(&tree).unwrap();
    let cancel = Cancel::new();
    process::checked(Some(&tree), &args(&["init", "--quiet"]), &cancel).unwrap();
    std::fs::write(tree.join(".gitignore"), b"listed/*\n!listed/visible.txt\n").unwrap();
    std::fs::create_dir_all(tree.join("listed/nested")).unwrap();
    let paths = watch_paths(&tree, &cancel).unwrap();
    assert!(paths.directories.contains(&PathBuf::from("listed")));
    assert!(paths.directories.contains(&PathBuf::from("listed/nested")));
    assert!(paths.ignored_directories.contains(&PathBuf::from("listed/nested")));
}

#[test]
fn configured_attributes_are_retained_as_monitoring_sources() {
    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path();
    let cancel = Cancel::new();
    process::checked(Some(tree), &args(&["init", "--quiet"]), &cancel).unwrap();
    process::checked(Some(tree), &args(&["config", "core.attributesFile", "ignored/rules"]), &cancel).unwrap();
    let sources = monitoring_sources(tree, &cancel).unwrap();
    assert!(sources.repository.iter().any(|source| crate::poll::normalize(source)
        == crate::poll::normalize(&tree.join("ignored/rules"))));
}

#[test]
fn git_configuration_scopes_keep_global_sources_shared() {
    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path().join("repo");
    let system = directory.path().join("system/config");
    let global = directory.path().join("global/config");
    let attributes = directory.path().join("global/attributes");
    let mut bytes = Vec::new();
    for (scope, origin, setting) in [
        ("system", system.clone(), "include.path\nsystem-include".to_owned()),
        ("global", global.clone(), format!("core.attributesfile\n{}", attributes.display())),
        ("local", tree.join(".git/config"), "include.path\nrepository-include".to_owned()),
        ("worktree", tree.join(".git/config.worktree"), "core.attributesfile\nrepository-attributes".to_owned()),
    ] {
        for field in [scope.to_owned(), format!("file:{}", origin.display()), setting] {
            bytes.extend(field.as_bytes());
            bytes.push(0);
        }
    }
    let sources = configuration_sources(&tree, &bytes).unwrap();
    for path in [system, global, directory.path().join("system/system-include"), attributes] {
        assert!(sources.global.contains(&path), "{}", path.display());
        assert!(!sources.repository.contains(&path));
    }
    for path in [tree.join(".git/config"), tree.join(".git/repository-include"),
        tree.join(".git/config.worktree"), tree.join("repository-attributes")] {
        assert!(sources.repository.contains(&path), "{}", path.display());
        assert!(!sources.global.contains(&path));
    }
}

#[test]
fn configured_local_excludes_retain_repository_scope() {
    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path();
    let cancel = Cancel::new();
    process::checked(Some(tree), &args(&["init", "--quiet"]), &cancel).unwrap();
    process::checked(Some(tree), &args(&["config", "core.excludesFile", "local-ignore"]), &cancel).unwrap();
    let sources = monitoring_sources(tree, &cancel).unwrap();
    assert!(sources.repository.contains(&tree.join("local-ignore")));
    assert!(!sources.global.contains(&tree.join("local-ignore")));
}
