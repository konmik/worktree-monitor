use super::*;
use super::policy::WorktreeEventPolicy;
use notify::event::ModifyKind;

fn registrations(targets: &WatchTargets) -> HashMap<PathBuf, Registration> {
    let mut plans = HashMap::new();
    for (key, path) in &targets.worktrees {
        if let Ok(paths) = git::watch_paths(path, &Cancel::new()) {
            plans.insert(key.clone(), Arc::new(plan_for(path, &paths)));
        }
    }
    registrations_with_plans(targets, &plans)
}

use notify::event::{CreateKind, DataChange, RemoveKind, RenameMode};

fn boundary_with(root: PathBuf, targets: Vec<Target>) -> WatchBoundary {
    let mut boundary = WatchBoundary::new(Cancel::new());
    boundary.roots.insert(normalize(&root), Registration { mode: RecursiveMode::Recursive, targets });
    boundary
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

#[path = "../filesystem-watchers-recover-after-failures/watch.test.rs"]
mod filesystem_watchers_recover_after_failures;

#[path = "../ignored-directories-do-not-cause-background-scans/watch.test.rs"]
mod ignored_directories_do_not_cause_background_scans;
