use super::*;

#[test]
fn metadata_events_survive_ignored_noise_in_the_same_batch() {
    let root = absolute(Path::new("watch-root"));
    let common = root.join(".git");
    let key = WorktreeKey::new(&common, &root);
    let ignored = normalize(&root.join("ignored"));
    let plan = Arc::new(WorktreeWatchPlan {
        recursive_roots: vec![normalize(&root)], directory_gates: Vec::new(),
        ignored_directories: HashSet::from([ignored]),
        tracked_files: HashSet::new(), tracked_directories: HashSet::new(),
    });
    let mut boundary = boundary_with(root.clone(), vec![
        Target::Worktree(key.clone()), Target::WorktreeMetadata(key.clone(), common.clone()),
    ]);
    boundary.plans.insert(key.clone(), plan);
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Create(CreateKind::File))
        .add_path(root.join("ignored/noise.txt")), &mut batch);
    boundary.classify(Event::new(EventKind::Modify(ModifyKind::Any))
        .add_path(common.join("index")), &mut batch);
    boundary.filter_ignored_batch(&mut batch);
    assert_eq!(batch.worktrees, HashSet::from([key]));
}

#[test]
fn full_recovery_reloads_ignore_rules_changed_while_disconnected() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("repo");
    std::fs::create_dir_all(root.join("ignored/nested")).unwrap();
    let cancel = Cancel::new();
    crate::process::checked(Some(&root), &crate::process::args(&["init", "--quiet"]), &cancel).unwrap();
    std::fs::write(root.join(".gitignore"), b"ignored/\n").unwrap();
    let key = WorktreeKey::new(&root.join(".git"), &root);
    let targets = WatchTargets {
        config: directory.path().join("config.json"), launch: None,
        repositories: Vec::new(), worktrees: vec![(key.clone(), root.clone())],
        global_excludes: Vec::new(),
    };
    let mut boundary = WatchBoundary::new(cancel);
    boundary.reconcile(&targets, Instant::now());
    let changed = root.join("ignored/nested/visible.txt");
    let kind = EventKind::Modify(ModifyKind::Data(DataChange::Content));
    assert!(!boundary.event_policy.read().unwrap().allows(&changed, &kind));
    boundary.watcher = None;
    std::fs::write(root.join(".gitignore"), b"").unwrap();
    boundary.sender.send(Err(notify::Error::generic("lost events"))).unwrap();
    assert!(boundary.drain(Instant::now()).full);
    assert!(!boundary.reconcile(&targets, Instant::now()).degraded);
    let policy = boundary.event_policy.read().unwrap().clone();
    assert!(policy.allows(&changed, &kind));
    std::fs::write(&changed, b"visible after recovery\n").unwrap();
    forward_event(Ok(Event::new(kind).add_path(changed)), &boundary.sender, &boundary.cancel, &policy);
    assert!(boundary.drain(Instant::now()).worktrees.contains(&key));
}

#[test]
fn planned_event_policy_preserves_tracked_exceptions_and_directory_changes() {
    let root = absolute(Path::new("watch-root"));
    let plan = plan_for(&root, &git::WatchPaths {
        directories: ["visible", "ignored", "ignored/nested", "ignored/noise"]
            .into_iter().map(PathBuf::from).collect(),
        ignored_directories: HashSet::from([PathBuf::from("ignored")]),
        tracked: ["visible/tracked.txt", "ignored/nested/tracked.txt"]
            .into_iter().map(PathBuf::from).collect(),
    });
    let policy = EventPolicy { worktrees: vec![WorktreeEventPolicy {
        key: WorktreeKey::new(&root.join(".git"), &root), root: normalize(&root),
        plan: Arc::new(plan), protected: Vec::new(),
    }] };
    let modified = EventKind::Modify(ModifyKind::Data(DataChange::Content));
    let removed = EventKind::Remove(RemoveKind::Folder);
    for (path, kind, allowed) in [
        ("visible/tracked.txt", modified, true),
        ("visible/untracked.txt", modified, true),
        ("ignored/nested/tracked.txt", modified, true),
        ("ignored/nested/untracked.txt", modified, false),
        ("ignored/nested/.gitignore", modified, true),
        ("ignored/noise/.gitignore", modified, false),
        ("ignored/nested", removed, true),
        ("ignored/noise", removed, false),
    ] {
        let event = Event::new(kind).add_path(root.join(path));
        assert_eq!(policy.filter(event).is_some(), allowed, "{path}");
    }
}

#[cfg(windows)]
#[test]
fn windows_callback_filters_ignored_events_before_waking_or_queuing() {
    let root = absolute(Path::new("watch-root"));
    let ignored = root.join("ignored");
    let tracked = ignored.join("tracked.txt");
    let plan = Arc::new(WorktreeWatchPlan {
        recursive_roots: vec![root.clone()], directory_gates: Vec::new(),
        ignored_directories: HashSet::from([normalize(&ignored)]),
        tracked_files: HashSet::from([normalize(&tracked)]),
        tracked_directories: HashSet::from([normalize(&ignored)]),
    });
    let key = WorktreeKey::new(&root.join(".git"), &root);
    let mut boundary = WatchBoundary::new(Cancel::new());
    boundary.plans.insert(key.clone(), plan);
    boundary.publish_policy(&WatchTargets {
        config: ignored.join("config.json"), launch: Some(root.clone()),
        repositories: vec![root.join(".git")], worktrees: vec![(key, root.clone())],
        global_excludes: Vec::new(),
    });
    let policy = boundary.event_policy.read().unwrap().clone();
    let cancel = Cancel::new();
    let (sender, receiver) = mpsc::channel();
    let generation = cancel.generation();
    for path in [ignored.join("noise.txt"), ignored.join("noise/.gitignore")] {
        forward_event(Ok(Event::new(EventKind::Create(CreateKind::File))
            .add_path(path)), &sender, &cancel, &policy);
    }
    assert_eq!(cancel.generation(), generation);
    assert!(receiver.try_recv().is_err());

    forward_event(Ok(Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
        .add_path(tracked.clone())), &sender, &cancel, &policy);
    assert_eq!(cancel.generation(), generation + 1);
    assert_eq!(receiver.try_recv().unwrap().unwrap().paths, vec![tracked]);

    for path in [ignored.join("config.json"), ignored.join(".gitattributes"), root.join(".git/HEAD")] {
        forward_event(Ok(Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(path.clone())), &sender, &cancel, &policy);
        assert_eq!(receiver.try_recv().unwrap().unwrap().paths, vec![path]);
    }

    let visible = root.join("visible.txt");
    forward_event(Ok(Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
        .add_path(ignored.join("noise.txt")).add_path(visible.clone())),
        &sender, &cancel, &policy);
    let event = receiver.try_recv().unwrap().unwrap();
    assert_eq!(event.paths, vec![visible]);
}

#[test]
fn failed_initial_plan_keeps_recursive_fallback_coverage() {
    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path().join("tree");
    std::fs::create_dir(&tree).unwrap();
    let common = tree.join(".git");
    let key = WorktreeKey::new(&common, &tree);
    let mut boundary = WatchBoundary::new(Cancel::new());
    let status = boundary.reconcile(&WatchTargets {
        config: directory.path().join("config.json"), launch: None,
        repositories: Vec::new(), worktrees: vec![(key.clone(), tree.clone())],
        global_excludes: Vec::new(),
    }, Instant::now());
    assert!(!status.degraded);
    assert_eq!(boundary.roots[&normalize(&tree)].mode, RecursiveMode::Recursive);
    boundary.sender.send(Ok(Event::new(EventKind::Modify(ModifyKind::Any))
        .add_path(tree.join("edited.txt")))).unwrap();
    let batch = boundary.drain(Instant::now());
    assert!(batch.worktrees.contains(&key));
}

#[test]
fn overlapping_worktree_policies_keep_changes_needed_by_either_repository() {
    let root = absolute(Path::new("watch-parent"));
    let child = root.join("child");
    let ignored = child.join("ignored");
    let changed = ignored.join("source.rs");
    let mut child_plan = fallback_plan(&child);
    child_plan.ignored_directories.insert(normalize(&ignored));
    let child_policy = WorktreeEventPolicy {
        key: WorktreeKey::new(&child.join(".git"), &child), root: normalize(&child),
        plan: Arc::new(child_plan), protected: Vec::new(),
    };
    let mut policy = EventPolicy { worktrees: vec![child_policy] };
    assert!(policy.filter(Event::new(EventKind::Modify(ModifyKind::Any))
        .add_path(changed.clone())).is_none());
    policy.worktrees.push(WorktreeEventPolicy {
        key: WorktreeKey::new(&root.join(".git"), &root), root: normalize(&root),
        plan: Arc::new(fallback_plan(&root)), protected: Vec::new(),
    });
    assert_eq!(policy.filter(Event::new(EventKind::Modify(ModifyKind::Any))
        .add_path(changed.clone())).unwrap().paths, vec![changed]);
}

#[test]
fn later_plan_failure_does_not_keep_stale_ignore_rules() {
    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path().join("tree");
    let ignored = tree.join("ignored");
    std::fs::create_dir_all(&ignored).unwrap();
    let changed = ignored.join("source.rs");
    let key = WorktreeKey::new(&tree.join(".git"), &tree);
    let mut plan = fallback_plan(&tree);
    plan.ignored_directories.insert(normalize(&ignored));
    let mut boundary = WatchBoundary::new(Cancel::new());
    boundary.plans.insert(key.clone(), Arc::new(plan));
    assert!(boundary.ignored_event(&normalize(&changed), &key, &EventKind::Modify(ModifyKind::Any)));
    boundary.refresh_plan(&key, &tree);
    assert!(boundary.plan_errors.contains(&key));
    boundary.publish_policy(&WatchTargets {
        config: directory.path().join("config.json"), launch: Some(tree.clone()),
        repositories: Vec::new(), worktrees: vec![(key, tree)], global_excludes: Vec::new(),
    });
    let policy = boundary.event_policy.read().unwrap().clone();
    assert_eq!(policy.filter(Event::new(EventKind::Modify(ModifyKind::Any))
        .add_path(changed.clone())).unwrap().paths, vec![changed]);
}

#[test]
fn ignored_directories_use_gates_and_never_recursive_native_roots() {
    let directory = tempfile::tempdir().unwrap();
    let repo = directory.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let cancel = Cancel::new();
    crate::process::checked(Some(&repo), &crate::process::args(&["init", "--quiet"]), &cancel).unwrap();
    std::fs::write(repo.join(".gitignore"), b"ignored/\n").unwrap();
    std::fs::create_dir_all(repo.join("ignored/nested")).unwrap();
    std::fs::create_dir_all(repo.join("allowed/nested/deep")).unwrap();
    std::fs::write(repo.join("ignored/nested/tracked.txt"), b"tracked\n").unwrap();
    crate::process::checked(Some(&repo), &crate::process::args(&["add", ".gitignore"]), &cancel).unwrap();
    crate::process::checked(Some(&repo), &crate::process::args(&["add", "-f", "ignored/nested/tracked.txt"]), &cancel).unwrap();
    let common = repo.join(".git");
    let key = WorktreeKey::new(&common, &repo);
    let roots = registrations(&WatchTargets {
        config: directory.path().join("config.json"),
        launch: None,
        repositories: vec![common],
        worktrees: vec![(key, repo.clone())],
        global_excludes: Vec::new(),
    });
    #[cfg(not(windows))]
    let ignored = normalize(&repo.join("ignored"));
    #[cfg(not(windows))]
    assert_eq!(roots[&ignored].mode, RecursiveMode::NonRecursive);
    #[cfg(not(windows))]
    assert_eq!(roots[&normalize(&repo.join("ignored/nested"))].mode, RecursiveMode::NonRecursive);
    #[cfg(not(windows))]
    assert_eq!(roots[&normalize(&repo.join("allowed"))].mode, RecursiveMode::Recursive);
    #[cfg(not(windows))]
    assert!(!roots.contains_key(&normalize(&repo.join("allowed/nested"))));
    #[cfg(not(windows))]
    assert!(roots[&normalize(&repo)].targets.iter().all(|target| matches!(target, Target::Worktree(_))));
    #[cfg(not(windows))]
    assert!(roots.iter().filter(|(_, registration)| registration.mode == RecursiveMode::Recursive)
        .all(|(path, _)| !path.starts_with(&ignored)));
    #[cfg(windows)]
    assert_eq!(roots[&normalize(&repo)].mode, RecursiveMode::Recursive);
    #[cfg(windows)]
    assert!(roots[&normalize(&repo)].targets.iter().any(|target| matches!(target, Target::Repository(_))));
    #[cfg(windows)]
    assert!(roots.keys().all(|path| *path == normalize(&repo) || !path.starts_with(&repo)));
    #[cfg(not(windows))]
    let recursive: Vec<_> = roots.iter().filter_map(|(path, registration)|
        (registration.mode == RecursiveMode::Recursive).then_some(path)).collect();
    #[cfg(not(windows))]
    assert!(recursive.iter().enumerate().all(|(index, path)| recursive.iter().enumerate()
        .all(|(other, other_path)| index == other || !path.starts_with(*other_path))));
}
