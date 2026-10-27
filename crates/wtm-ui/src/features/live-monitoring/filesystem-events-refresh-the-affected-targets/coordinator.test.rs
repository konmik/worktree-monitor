use super::*;

#[test]
fn worktree_events_only_queue_the_changed_worktree() {
    let latest = Arc::new(Mutex::new(None));
    let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
    let first = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo/first"));
    let second = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo/second"));
    let mut batch = watch::WatchBatch::default();
    batch.worktrees.insert(first.clone());
    coordinator.enqueue(batch);
    assert!(coordinator.passes.changed.contains_key(&first));
    assert!(!coordinator.passes.changed.contains_key(&second));
}

#[test]
fn monitor_repair_refreshes_counts_after_missed_events() {
    let (_directory, mut coordinator) = scan_fixture(1);
    coordinator.scan_pass();
    let key = coordinator.worktrees.keys().next().unwrap().clone();
    assert_eq!(coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts.source.additions, 0);
    coordinator.watch = watch::WatchBoundary::new(coordinator.cancel.clone());
    coordinator.discoveries.clear();
    coordinator.passes.changed.clear();
    std::fs::write(coordinator.worktrees[&key].path.join("missed.txt"), b"one\ntwo\n").unwrap();
    coordinator.enqueue(watch::WatchBatch { full: true, ..Default::default() });
    assert!(coordinator.passes.changed.contains_key(&key));
    coordinator.scan_pass();
    assert_eq!(coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts.source.additions, 2);
}

#[test]
fn taking_a_pass_preserves_new_events_for_the_next_pass() {
    let mut passes = ScanPasses::default();
    let key = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo"));
    passes.changed.insert(key.clone(), PendingScan::Forced);
    let current = std::mem::take(&mut passes.changed);
    passes.changed.insert(key.clone(), PendingScan::Forced);
    assert!(current.contains_key(&key));
    assert!(passes.changed.contains_key(&key));
}
