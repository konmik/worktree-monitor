use super::*;

fn registrations(targets: &WatchTargets) -> HashMap<PathBuf, Registration> {
    let mut plans = HashMap::new();
    for (key, path) in &targets.worktrees {
        if let Ok(paths) = git::watch_paths(path, &Cancel::new()) {
            plans.insert(key.clone(), Arc::new(plan_for(path, &paths)));
        }
    }
    registrations_with_plans(targets, &plans)
}use notify::event::{CreateKind, DataChange, RemoveKind, RenameMode};

fn boundary_with(root: PathBuf, targets: Vec<Target>) -> WatchBoundary {
    let mut boundary = WatchBoundary::new(Cancel::new());
    boundary.roots.insert(normalize(&root), Registration { mode: RecursiveMode::Recursive, targets });
    boundary
}

#[test]
fn config_sibling_events_are_ignored() {
    let root = PathBuf::from("watch-root");
    let config = absolute(Path::new("watch-root/config.json"));
    let mut boundary = boundary_with(root.clone(), vec![Target::Config(config.clone())]);
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Create(CreateKind::File))
        .add_path(absolute(Path::new("watch-root/config.order.json"))), &mut batch);
    assert!(!batch.config);
    boundary.classify(Event::new(EventKind::Create(CreateKind::File)).add_path(config), &mut batch);
    assert!(batch.config);
}

#[test]
fn linked_worktree_root_replacement_marks_it_changed() {
    let root = absolute(Path::new("watch-root/linked"));
    let common = absolute(Path::new("watch-root/common.git"));
    let key = WorktreeKey::new(&common, &root);
    let mut boundary = boundary_with(root.clone(), vec![Target::Worktree(key.clone())]);
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Remove(RemoveKind::Folder)).add_path(root), &mut batch);
    assert!(batch.worktrees.contains(&key));
    assert!(batch.topology);
}

#[test]
fn worktree_admin_create_requests_discovery_but_ref_edit_does_not() {
    let common = absolute(Path::new("watch-root/.git"));
    let key = WorktreeKey::new(&common, Path::new("watch-root"));
    let mut boundary = boundary_with(common.clone(), vec![Target::Repository(common.clone())]);
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Create(CreateKind::Folder))
        .add_path(common.join("worktrees/new")), &mut batch);
    assert_eq!(batch.discoveries.len(), 1);
    assert!(batch.repositories.is_empty());
    let mut boundary = boundary_with(common.clone(), vec![Target::Repository(common)]);
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Remove(RemoveKind::File))
        .add_path(absolute(Path::new("watch-root/.git/refs/heads/main"))), &mut batch);
    assert!(batch.discoveries.is_empty());
    assert!(batch.repositories.len() == 1);
    assert!(key.path.ends_with("watch-root"));
}

#[test]
fn object_store_writes_do_not_invalidate_worktree_counts() {
    let tree = absolute(Path::new("watch-root"));
    let common = tree.join(".git");
    let key = WorktreeKey::new(&common, &tree);
    let mut boundary = boundary_with(tree, vec![
        Target::Repository(common.clone()), Target::Worktree(key),
    ]);
    for kind in [EventKind::Create(CreateKind::File),
        EventKind::Modify(ModifyKind::Any)] {
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(kind).add_path(common.join("objects/e6/empty")), &mut batch);
        assert!(batch.is_empty());
    }
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Modify(ModifyKind::Any))
        .add_path(common.join("refs/heads/main")), &mut batch);
    assert!(batch.repositories.contains(&normalize(&common)));
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Remove(RemoveKind::Folder))
        .add_path(common.join("objects")), &mut batch);
    assert!(batch.repositories.contains(&normalize(&common)));
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Modify(ModifyKind::Any))
        .add_path(common.join("objects/info/alternates")), &mut batch);
    assert!(batch.repositories.contains(&normalize(&common)));
}

#[test]
fn directory_events_mark_the_worktree_changed() {
    let directory = tempfile::Builder::new().prefix("wtm-watch-ignore-").tempdir().unwrap();
    let root = directory.path();
    let cancel = Cancel::new();
    crate::process::checked(Some(root), &crate::process::args(&["init", "--quiet"]), &cancel).unwrap();
    std::fs::write(root.join(".gitignore"), b"ignored/\n").unwrap();
    std::fs::write(root.join("visible.txt"), b"visible\n").unwrap();
    std::fs::create_dir(root.join("ignored")).unwrap();
    let common = root.join(".git");
    let key = WorktreeKey::new(&common, root);
    let mut boundary = boundary_with(root.to_path_buf(), vec![
        Target::Repository(common), Target::Worktree(key.clone()),
    ]);
    for kind in [EventKind::Create(CreateKind::File), EventKind::Modify(ModifyKind::Any),
        EventKind::Remove(RemoveKind::File), EventKind::Modify(ModifyKind::Name(RenameMode::From))] {
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(kind).add_path(root.join("ignored/artifact.txt")), &mut batch);
        assert!(batch.worktrees.contains(&key));
    }
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Modify(ModifyKind::Any))
        .add_path(root.join("visible.txt")), &mut batch);
    assert!(batch.worktrees.contains(&key));
}

#[test]
fn parent_directory_modifications_do_not_mark_child_worktrees() {
    let parent = absolute(Path::new("watch-parent"));
    let root = parent.join("repo");
    let common = root.join(".git");
    let key = WorktreeKey::new(&common, &root);
    let mut boundary = boundary_with(parent.clone(), vec![
        Target::Repository(common), Target::Worktree(key),
    ]);
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Modify(ModifyKind::Any)).add_path(parent), &mut batch);
    assert!(batch.is_empty());
}

#[test]
fn index_events_only_mark_the_owning_worktree() {
    let common = absolute(Path::new("watch-root/.git"));
    let main = WorktreeKey::new(&common, Path::new("watch-root"));
    let linked = WorktreeKey::new(&common, Path::new("linked-root"));
    let admin = common.join("worktrees/linked");
    let mut boundary = boundary_with(common.clone(), vec![
        Target::Repository(common.clone()),
        Target::WorktreeMetadata(main.clone(), common.clone()),
        Target::WorktreeMetadata(linked.clone(), admin.clone()),
    ]);
    for (path, expected) in [(common.join("index"), &main), (admin.join("index"), &linked)] {
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Modify(ModifyKind::Any)).add_path(path), &mut batch);
        assert_eq!(batch.worktrees, HashSet::from([expected.clone()]));
        assert!(batch.repositories.is_empty());
    }
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
        .add_path(common.join("HEAD")), &mut batch);
    assert_eq!(batch.worktrees, HashSet::from([main.clone()]));
    assert!(batch.plans.is_empty());
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Modify(ModifyKind::Any))
        .add_path(common.join("refs/heads/main")), &mut batch);
    assert!(batch.repositories.contains(&normalize(&common)));
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Create(CreateKind::File))
        .add_path(common.join("packed-refs.lock")), &mut batch);
    boundary.classify(Event::new(EventKind::Remove(RemoveKind::File))
        .add_path(common.join("packed-refs.lock")), &mut batch);
    assert!(batch.is_empty());
}

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
fn launch_events_only_follow_git_markers_or_directory_topology() {
    let launch = absolute(Path::new("watch-root/nested/deeper"));
    assert!(launch_event(&launch.join(".git"), &launch,
        &EventKind::Create(CreateKind::Folder)));
    assert!(launch_event(&absolute(Path::new("watch-root/.git")), &launch,
        &EventKind::Modify(ModifyKind::Data(DataChange::Content))));
    assert!(launch_event(&absolute(Path::new("watch-root/nested")), &launch,
        &EventKind::Remove(RemoveKind::Folder)));
    assert!(!launch_event(&launch.join("ordinary.txt"), &launch,
        &EventKind::Create(CreateKind::File)));
    assert!(!launch_event(&launch.join(".gitignore"), &launch,
        &EventKind::Modify(ModifyKind::Data(DataChange::Content))));
    assert!(!launch_event(&launch.join("child/.git"), &launch,
        &EventKind::Create(CreateKind::Folder)));
}

#[test]
fn repository_metadata_root_replacement_reattaches_without_discovery() {
    let common = absolute(Path::new("watch-root/.git"));
    let refs = common.join("refs");
    let mut boundary = boundary_with(common.clone(), vec![Target::Repository(common.clone())]);
    boundary.roots.insert(normalize(&refs), Registration {
        mode: RecursiveMode::Recursive,
        targets: vec![Target::Repository(common.clone())],
    });
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
        .add_path(refs.clone()), &mut batch);
    assert!(batch.topology);
    assert!(batch.discoveries.is_empty());
    assert!(batch.repositories.contains(&normalize(&common)));
    assert!(!boundary.roots.contains_key(&normalize(&refs)));

    let mut boundary = boundary_with(common.clone(), vec![Target::Repository(common.clone())]);
    boundary.roots.insert(normalize(&refs), Registration {
        mode: RecursiveMode::Recursive,
        targets: vec![Target::Repository(common.clone())],
    });
    let mut batch = WatchBatch::default();
    boundary.classify(Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
        .add_path(refs.join("heads/main")), &mut batch);
    assert!(!batch.topology);
    assert!(batch.discoveries.is_empty());
    assert!(boundary.roots.contains_key(&normalize(&refs)));
}

#[test]
fn watcher_errors_request_monitor_repair() {
    let mut boundary = WatchBoundary::new(Cancel::new());
    boundary.sender.send(Err(notify::Error::generic("watch failed"))).unwrap();
    let now = Instant::now();
    let batch = boundary.drain(now);
    assert!(batch.full);
    assert_eq!(boundary.next_retry(), Some(now));
    assert!(boundary.degraded);
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
fn replacing_native_watch_mode_requests_full_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let root = normalize(directory.path());
    let config = directory.path().join("config.json");
    let mut boundary = WatchBoundary::new(Cancel::new());
    boundary.watcher.as_mut().unwrap().watch(&root, RecursiveMode::Recursive).unwrap();
    boundary.roots.insert(root.clone(), Registration {
        mode: RecursiveMode::Recursive, targets: vec![Target::Config(config.clone())],
    });
    let status = boundary.reconcile(&WatchTargets {
        config, launch: None, repositories: Vec::new(), worktrees: Vec::new(),
        global_excludes: Vec::new(),
    }, Instant::now());
    assert!(status.full);
    assert!(!status.degraded);
    assert_eq!(boundary.roots[&root].mode, RecursiveMode::NonRecursive);
}

#[test]
fn forced_rebuild_reports_full_reconcile() {
    let mut boundary = WatchBoundary::new(Cancel::new());
    boundary.force_rebuild = true;
    let now = Instant::now();
    let status = boundary.reconcile(&WatchTargets {
        config: PathBuf::from("config.json"),
        launch: None,
        repositories: Vec::new(),
        worktrees: Vec::new(),
        global_excludes: Vec::new(),
    }, now);
    assert!(status.full);
    assert!(!status.degraded);
    assert_eq!(boundary.next_retry(), None);
}

#[test]
fn drain_limits_events_and_wakes_for_queued_remainder() {
    let cancel = Cancel::new();
    let mut boundary = WatchBoundary::new(cancel.clone());
    for index in 0..=MAX_DRAIN_EVENTS {
        let event = Event::new(EventKind::Create(CreateKind::File))
            .add_path(PathBuf::from(format!("watch-root/{index}")));
        boundary.sender.send(Ok(event)).unwrap();
    }
    let generation = cancel.generation();
    let batch = boundary.drain(Instant::now());
    assert!(batch.is_empty());
    assert_eq!(cancel.generation(), generation + 1);
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
fn deferred_plans_keep_nested_startup_events_without_git_planning() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("repo");
    std::fs::create_dir_all(root.join("nested/deep")).unwrap();
    let key = WorktreeKey::new(&root.join(".git"), &root);
    let targets = WatchTargets {
        config: directory.path().join("config.json"), launch: None,
        repositories: Vec::new(), worktrees: vec![(key.clone(), root.clone())],
        global_excludes: Vec::new(),
    };
    let mut boundary = WatchBoundary::new(Cancel::new());
    boundary.before_plan = Some(Box::new(|| panic!("detailed planning ran before initial counts")));
    assert!(!boundary.prepare(&targets, Instant::now()).degraded);
    let changed = root.join("nested/deep/startup.txt");
    std::fs::write(&changed, b"startup\n").unwrap();
    boundary.sender.send(Ok(Event::new(EventKind::Create(CreateKind::File))
        .add_path(changed))).unwrap();
    let batch = boundary.drain(Instant::now());
    assert!(batch.worktrees.contains(&key));
    assert!(boundary.plan_dirty.contains(&key));
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

#[test]
fn existing_repository_and_worktree_use_narrow_roots() {
    let directory = tempfile::tempdir().unwrap();
    let repo = directory.path().join("repo");
    let common = repo.join(".git");
    std::fs::create_dir(&repo).unwrap();
    crate::process::checked(Some(&repo), &crate::process::args(&["init", "--quiet"]), &Cancel::new()).unwrap();
    for name in ["refs", "worktrees", "info"] {
        std::fs::create_dir_all(common.join(name)).unwrap();
    }
    let key = WorktreeKey::new(&common, &repo);
    let roots = registrations(&WatchTargets {
        config: directory.path().join("config.json"),
        launch: None,
        repositories: vec![common.clone()],
        worktrees: vec![(key, repo.clone())],
        global_excludes: vec![common.join("config")],
    });
    assert_eq!(roots[&normalize(directory.path())].mode, RecursiveMode::NonRecursive);
    #[cfg(windows)]
    {
        assert_eq!(roots[&normalize(&repo)].mode, RecursiveMode::Recursive);
        assert!(!roots.contains_key(&normalize(&common)));
        for name in ["refs", "worktrees", "info"] {
            assert!(!roots.contains_key(&normalize(&common.join(name))));
        }
        assert!(roots[&normalize(&repo)].targets.iter()
            .any(|target| matches!(target, Target::Repository(_))));
    }
    #[cfg(not(windows))]
    {
        assert_eq!(roots[&normalize(&repo)].mode, RecursiveMode::NonRecursive);
        assert_eq!(roots[&normalize(&common)].mode, RecursiveMode::NonRecursive);
        for name in ["refs", "worktrees", "info"] {
            assert_eq!(roots[&normalize(&common.join(name))].mode, RecursiveMode::Recursive);
        }
    }
}

#[test]
fn existing_launch_uses_an_exact_nonrecursive_root() {
    let directory = tempfile::tempdir().unwrap();
    let launch = directory.path().join("project/nested");
    std::fs::create_dir_all(&launch).unwrap();
    let roots = registrations(&WatchTargets {
        config: directory.path().join("config.json"),
        launch: Some(launch.clone()),
        repositories: Vec::new(),
        worktrees: Vec::new(),
        global_excludes: Vec::new(),
    });
    let registration = &roots[&normalize(&launch)];
    assert_eq!(registration.mode, RecursiveMode::NonRecursive);
    assert!(registration.targets.iter().any(|target| {
        matches!(target, Target::Launch(path) if normalize(path) == normalize(&launch))
    }));
}

#[cfg(windows)]
#[test]
fn windows_merges_nested_registrations_onto_the_worktree_root() {
    let directory = tempfile::tempdir().unwrap();
    let repo = directory.path().join("repo");
    let common = repo.join(".git");
    std::fs::create_dir(&repo).unwrap();
    crate::process::checked(Some(&repo), &crate::process::args(&["init", "--quiet"]), &Cancel::new()).unwrap();
    for name in ["refs", "worktrees", "info"] {
        std::fs::create_dir_all(common.join(name)).unwrap();
    }
    let key = WorktreeKey::new(&common, &repo);
    let mut boundary = WatchBoundary::new(Cancel::new());
    let status = boundary.reconcile(&WatchTargets {
        config: directory.path().join("config.json"),
        launch: None,
        repositories: vec![common.clone()],
        worktrees: vec![(key, repo.clone())],
        global_excludes: Vec::new(),
    }, Instant::now());
    assert!(!status.degraded);
    assert_eq!(boundary.roots[&normalize(&repo)].mode, RecursiveMode::Recursive);
    assert!(!boundary.roots.contains_key(&normalize(&common)));
    for name in ["refs", "worktrees", "info"] {
        assert!(!boundary.roots.contains_key(&normalize(&common.join(name))));
    }
    assert!(boundary.roots[&normalize(&repo)].targets.iter().any(|target| matches!(target, Target::Repository(_))));
}

#[cfg(windows)]
#[test]
fn windows_exact_worktree_registration_allows_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let repo = directory.path().join("repo");
    let common = repo.join(".git");
    std::fs::create_dir(&repo).unwrap();
    crate::process::checked(Some(&repo), &crate::process::args(&["init", "--quiet"]), &Cancel::new()).unwrap();
    for name in ["refs", "worktrees", "info"] {
        std::fs::create_dir_all(common.join(name)).unwrap();
    }
    let key = WorktreeKey::new(&common, &repo);
    let mut boundary = WatchBoundary::new(Cancel::new());
    let status = boundary.reconcile(&WatchTargets {
        config: directory.path().join("config.json"),
        launch: None,
        repositories: vec![common],
        worktrees: vec![(key, repo.clone())],
        global_excludes: Vec::new(),
    }, Instant::now());
    assert!(!status.degraded);
    boundary.watcher = None;
    let moved = directory.path().join("moved");
    std::fs::rename(&repo, &moved).unwrap();
    std::fs::rename(moved, repo).unwrap();
}
