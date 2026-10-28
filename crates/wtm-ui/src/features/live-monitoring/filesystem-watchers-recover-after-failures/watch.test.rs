use super::*;

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
fn replacing_native_watch_mode_requests_full_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let root = normalize(directory.path());
    let config = directory.path().join("config.json");
    let mut boundary = WatchBoundary::new(Cancel::new());
    boundary.watcher.as_mut().unwrap().watch(&root, RecursiveMode::Recursive).unwrap();
    boundary.roots.insert(root.clone(), Registration {
        mode: RecursiveMode::Recursive, targets: vec![Target::Config(config.clone())],
    });
    let status = boundary.rebuild_plans(&WatchTargets {
        config, launch: None, repositories: Vec::new(), worktrees: Vec::new(),
        global_excludes: Vec::new(),
        repository_sources: Vec::new(),
    }, Instant::now(), &HashSet::new());
    assert!(status.full);
    assert!(!status.degraded);
    assert_eq!(boundary.roots[&root].mode, RecursiveMode::NonRecursive);
}

#[test]
fn forced_rebuild_reports_full_reconcile() {
    let mut boundary = WatchBoundary::new(Cancel::new());
    boundary.force_rebuild = true;
    let now = Instant::now();
    let status = boundary.rebuild_plans(&WatchTargets {
        config: PathBuf::from("config.json"),
        launch: None,
        repositories: Vec::new(),
        worktrees: Vec::new(),
        global_excludes: Vec::new(),
        repository_sources: Vec::new(),
    }, now, &HashSet::new());
    assert!(status.full);
    assert!(!status.degraded);
    assert_eq!(boundary.next_retry(), None);
}

#[test]
fn recovered_plans_clear_pending_work_without_changing_subscriptions() {
    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path().join("tree");
    std::fs::create_dir(&tree).unwrap();
    let key = WorktreeKey::new(&tree.join(".git"), &tree);
    let targets = WatchTargets {
        config: directory.path().join("config.json"), launch: None,
        repositories: Vec::new(), worktrees: vec![(key.clone(), tree.clone())],
        global_excludes: Vec::new(),
        repository_sources: Vec::new(),
    };
    let mut boundary = WatchBoundary::new(Cancel::new());
    let now = Instant::now();
    assert!(!boundary.rebuild_plans(&targets, now, &HashSet::from([key.clone()])).degraded);
    assert!(boundary.has_pending_plans());
    assert_eq!(boundary.next_retry(), None);
    let retry = now + std::time::Duration::from_secs(10);
    #[cfg(windows)]
    let subscriptions = boundary.roots.clone();
    crate::process::checked(Some(&tree), &crate::process::args(&["init", "--quiet"]), &Cancel::new()).unwrap();
    assert!(!boundary.rebuild_plans(&targets, retry, &HashSet::from([key.clone()])).degraded);
    assert!(!boundary.has_pending_plans());
    #[cfg(windows)]
    assert_eq!(boundary.roots, subscriptions);
    assert_eq!(boundary.next_retry(), None);
    assert!(!boundary.rebuild_plans(&targets, retry + std::time::Duration::from_secs(1), &HashSet::from([key.clone()])).degraded);
    assert_eq!(boundary.next_retry(), None);
    boundary.sender.send(Ok(Event::new(EventKind::Modify(ModifyKind::Any))
        .add_path(tree.join("edited.txt")))).unwrap();
    assert!(boundary.drain(retry).worktrees.contains(&key));
}

#[test]
fn failed_plans_stay_pending_without_native_repair_retries() {
    let directory = tempfile::tempdir().unwrap();
    let tree = directory.path().join("tree");
    std::fs::create_dir(&tree).unwrap();
    let key = WorktreeKey::new(&tree.join(".git"), &tree);
    let targets = WatchTargets {
        config: directory.path().join("config.json"), launch: None,
        repositories: Vec::new(), worktrees: vec![(key.clone(), tree.clone())],
        global_excludes: Vec::new(),
        repository_sources: Vec::new(),
    };
    let mut boundary = WatchBoundary::new(Cancel::new());
    let now = Instant::now();
    assert!(!boundary.rebuild_plans(&targets, now, &HashSet::from([key.clone()])).degraded);
    assert!(boundary.has_pending_plans());
    assert_eq!(boundary.next_retry(), None);
    let retry = now + std::time::Duration::from_secs(5);
    boundary.before_plan = Some(Box::new(|| panic!("native reconciliation retried Git planning")));
    assert!(!boundary.reconcile(&targets, retry).degraded);
    assert!(boundary.has_pending_plans());
    assert_eq!(boundary.next_retry(), None);
    boundary.sender.send(Ok(Event::new(EventKind::Modify(ModifyKind::Any))
        .add_path(tree.join("edited.txt")))).unwrap();
    assert!(boundary.drain(retry).worktrees.contains(&key));
}
