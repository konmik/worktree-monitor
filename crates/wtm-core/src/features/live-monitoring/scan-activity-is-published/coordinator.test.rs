use super::*;

#[test]
fn actual_scans_publish_starts_results_and_pending_descriptors() {
    let (_directory, mut coordinator) = scan_fixture(1);
    let original = coordinator.snapshot.clone();
    coordinator.scan_pass();
    let updates = &coordinator.published;
    assert_eq!(updates.len(), 4);
    let discovery_start = &updates[0];
    assert!(Arc::ptr_eq(&original, &discovery_start.snapshot));
    assert_eq!(discovery_start.scan_targets.len(), 1);
    assert_eq!(discovery_start.scan_activities.len(), 1);
    assert!(discovery_start.scan_activities[0].finished_at.is_none());
    let discovery_end = &updates[1];
    assert!(discovery_end.snapshot.repos.is_empty());
    assert_eq!(discovery_end.scan_targets.len(), 2);
    assert!(discovery_end.scan_activities[0].finished_at.is_some());
    assert!(!coordinator.repos.values().next().unwrap().sources.is_empty());
    let tree_start = &updates[2];
    assert!(Arc::ptr_eq(&discovery_end.snapshot, &tree_start.snapshot));
    assert_eq!(tree_start.scan_activities.len(), 2);
    assert_eq!(tree_start.scan_activities[0], discovery_end.scan_activities[0]);
    assert!(tree_start.scan_activities[1].finished_at.is_none());
    let tree_end = &updates[3];
    assert_eq!(tree_end.snapshot.repos[0].worktrees.as_ref().unwrap().len(), 1);
    assert!(tree_end.scan_activities.iter().all(|activity| activity.finished_at.is_some_and(|end| end >= activity.started_at)));
    assert_eq!(tree_start.scan_activities[0], tree_end.scan_activities[0]);
    assert_eq!(tree_start.scan_activities[1], ScanActivity { finished_at: None, ..tree_end.scan_activities[1].clone() });
}

#[test]
fn fast_completed_activity_is_retained_and_unchanged_scans_are_isolated() {
    let (_directory, mut coordinator) = scan_fixture(2);
    coordinator.scan_pass();
    let snapshot = coordinator.snapshot.clone();
    let common = coordinator.repo_order[0].clone();
    let key = coordinator.repos[&common].worktrees[0].clone();
    let other = coordinator.repos[&coordinator.repo_order[1]].activity.clone().unwrap();
    let previous = coordinator.worktrees[&key].activity.clone().unwrap();
    let repository = coordinator.repos[&common].activity.clone();
    coordinator.discoveries.clear();
    coordinator.passes.changed.clear();
    coordinator.passes.changed.insert(key.clone());
    coordinator.scan_pass();
    assert!(Arc::ptr_eq(&snapshot, &coordinator.snapshot));
    assert!(coordinator.worktrees[&key].activity.as_ref().unwrap().generation > previous.generation);
    assert_eq!(coordinator.repos[&common].activity, repository);
    assert_eq!(coordinator.repos[&coordinator.repo_order[1]].activity.as_ref(), Some(&other));
    let completed = coordinator.latest.lock().unwrap().clone().unwrap();
    coordinator.publish();
    coordinator.publish();
    let retained = coordinator.latest.lock().unwrap().clone().unwrap();
    assert_eq!(completed.scan_activities, retained.scan_activities);
    assert_eq!(retained.scan_targets.len(), 4);
    assert_eq!(retained.scan_activities.len(), 4);
    assert!(retained.scan_activities.iter().all(|activity| activity.finished_at.is_some()));
}

#[test]
fn queued_work_has_no_activity_and_removing_targets_removes_activity() {
    let (_directory, mut coordinator) = scan_fixture(1);
    coordinator.publish();
    assert!(coordinator.published.last().unwrap().scan_activities.is_empty());
    coordinator.scan_pass();
    let key = coordinator.worktrees.keys().next().unwrap().clone();
    coordinator.enqueue(watch::WatchBatch { worktrees: HashSet::from([key]), ..Default::default() });
    coordinator.publish();
    assert!(coordinator.published.last().unwrap().scan_activities.iter().all(|activity| activity.finished_at.is_some()));
    coordinator.registrations.repos.clear();
    coordinator.refresh_repositories();
    coordinator.publish();
    let update = coordinator.published.last().unwrap();
    assert!(update.scan_targets.is_empty());
    assert!(update.scan_activities.is_empty());
}

#[test]
fn removing_a_worktree_removes_its_retained_activity() {
    let (_directory, mut coordinator) = scan_fixture(1);
    coordinator.scan_pass();
    let common = coordinator.repo_order[0].clone();
    let path = coordinator.repos[&common].path.clone();
    coordinator.apply_discovery(&common, Ok(git::DiscoveredRepo { path, trees: Vec::new() }));
    coordinator.publish();
    let update = coordinator.published.last().unwrap();
    assert_eq!(update.scan_targets.len(), 1);
    assert_eq!(update.scan_activities.len(), 1);
    assert!(matches!(update.scan_activities[0].target, ScanTarget::Repository { .. }));
}

#[test]
fn cancelled_coordinator_finishes_active_activity_promptly() {
    let (_directory, mut coordinator) = scan_fixture(1);
    let common = coordinator.repo_order[0].clone();
    coordinator.start_scan(&common, None);
    let latest = coordinator.latest.clone();
    let active = latest.lock().unwrap().clone().unwrap();
    assert_eq!(active.scan_activities.len(), 1);
    assert!(active.scan_activities[0].finished_at.is_none());
    coordinator.cancel.cancel();
    let start = Instant::now();
    coordinator.run();
    assert!(start.elapsed() < Duration::from_secs(1));
    let update = latest.lock().unwrap().clone().unwrap();
    assert_eq!(update.scan_activities.len(), 1);
    assert!(update.scan_activities.iter().all(|activity| activity.finished_at.is_some()));
}
