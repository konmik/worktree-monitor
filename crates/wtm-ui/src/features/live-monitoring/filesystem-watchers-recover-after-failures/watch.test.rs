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
