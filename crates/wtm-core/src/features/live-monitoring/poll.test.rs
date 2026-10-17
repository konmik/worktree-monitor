use super::*;

fn scan_fixture(count: usize) -> (tempfile::TempDir, Coordinator) {
    let directory = tempfile::tempdir().unwrap();
    let mut repos = Vec::new();
    for index in 0..count {
        let path = directory.path().join(format!("repo-{index}"));
        std::fs::create_dir(&path).unwrap();
        crate::process::checked(Some(&path), &crate::process::args(&["init", "--quiet"]), &Cancel::new()).unwrap();
        repos.push(path.join(".git"));
    }
    let config = directory.path().join("config.json");
    let bytes = serde_json::to_vec(&serde_json::json!({ "repos": repos })).unwrap();
    std::fs::write(&config, &bytes).unwrap();
    let mut coordinator = Coordinator::new(Service::at(config), None, Cancel::new(), Arc::new(Mutex::new(None)));
    coordinator.registrations.reload(Ok(bytes));
    coordinator.refresh_repositories();
    coordinator.launch_changed = false;
    (directory, coordinator)
}

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
fn repository_discovery_does_not_refresh_existing_worktrees() {
    let (_directory, mut coordinator) = scan_fixture(2);
    coordinator.scan_pass();
    let common = coordinator.repo_order[0].clone();
    let previous = coordinator.repos[&common].activity.clone().unwrap();
    let trees: HashMap<_, _> = coordinator.worktrees.iter().map(|(key, state)| (key.clone(), state.activity.clone())).collect();
    let other = coordinator.repos[&coordinator.repo_order[1]].activity.clone();
    coordinator.discoveries.clear();
    coordinator.passes.changed.clear();
    coordinator.discoveries.insert(common.clone());
    let published = coordinator.published.len();
    coordinator.scan_pass();
    assert_eq!(coordinator.published.len() - published, 2);
    let start = &coordinator.published[published];
    let active: Vec<_> = start.scan_activities.iter().filter(|activity| activity.finished_at.is_none()).collect();
    assert_eq!(active.len(), 1);
    assert!(matches!(&active[0].target, ScanTarget::Repository { common_dir, .. } if normalize(common_dir) == common));
    assert!(active[0].generation > previous.generation);
    assert!(coordinator.repos[&common].activity.as_ref().unwrap().finished_at.is_some());
    assert_eq!(coordinator.repos[&coordinator.repo_order[1]].activity, other);
    for (key, activity) in trees { assert_eq!(coordinator.worktrees[&key].activity, activity); }
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

#[test]
fn passes_wait_ten_seconds_after_completion_without_sliding() {
    let mut passes = ScanPasses::default();
    let now = Instant::now();
    assert!(passes.ready(now));
    passes.finish(now);
    let key = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo"));
    for second in 1..10 {
        passes.changed.insert(key.clone());
        assert!(!passes.ready(now + Duration::from_secs(second)));
    }
    assert!(passes.ready(now + PASS_DELAY));
    assert_eq!(passes.changed.len(), 1);
}

#[test]
fn worktree_events_only_queue_the_changed_worktree() {
    let latest = Arc::new(Mutex::new(None));
    let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
    let first = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo/first"));
    let second = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo/second"));
    let mut batch = watch::WatchBatch::default();
    batch.worktrees.insert(first.clone());
    coordinator.enqueue(batch);
    assert!(coordinator.passes.changed.contains(&first));
    assert!(!coordinator.passes.changed.contains(&second));
}

#[test]
fn taking_a_pass_preserves_new_events_for_the_next_pass() {
    let mut passes = ScanPasses::default();
    let key = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo"));
    passes.changed.insert(key.clone());
    let current = std::mem::take(&mut passes.changed);
    passes.changed.insert(key.clone());
    assert!(current.contains(&key));
    assert!(passes.changed.contains(&key));
}

#[test]
fn startup_publishes_counts_before_detailed_plans_and_retains_edits_during_planning() {
    let (directory, mut coordinator) = scan_fixture(2);
    let latest = coordinator.latest.clone();
    let changed = directory.path().join("repo-0/startup.txt");
    let edited = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let edited_by_plan = edited.clone();
    coordinator.watch.before_plan = Some(Box::new(move || {
        let update = latest.lock().unwrap().clone().unwrap();
        assert_eq!(update.snapshot.repos.len(), 2);
        assert!(update.snapshot.repos.iter().all(|repo| repo.worktrees.as_ref().is_ok_and(|trees|
            trees.len() == 1 && trees[0].result.is_ok())));
        if !edited_by_plan.swap(true, std::sync::atomic::Ordering::SeqCst) {
            std::fs::write(&changed, b"during startup\n").unwrap();
        }
    }));
    coordinator.scan_pass();
    assert!(edited.load(std::sync::atomic::Ordering::SeqCst));
    let common = coordinator.repo_order[0].clone();
    let key = coordinator.repos[&common].worktrees[0].clone();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !coordinator.passes.changed.contains(&key) && Instant::now() < deadline {
        coordinator.receive_watch();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(coordinator.passes.changed.contains(&key));
    coordinator.scan_pass();
    assert_eq!(coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts.source.additions, 1);
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
    assert!(coordinator.passes.changed.contains(&key));
    coordinator.scan_pass();
    assert_eq!(coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts.source.additions, 2);
}

#[test]
fn discovery_schedules_new_worktrees_once_without_file_events() {
    let directory = tempfile::tempdir().unwrap();
    crate::process::checked(Some(directory.path()), &crate::process::args(&["init", "--quiet"]), &Cancel::new()).unwrap();
    let common_dir = directory.path().join(".git");
    let common = normalize(&common_dir);
    let latest = Arc::new(Mutex::new(None));
    let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
    coordinator.repos.insert(common.clone(), RepoState {
        common_dir, path: directory.path().to_path_buf(), discovered: None,
        worktrees: Vec::new(), sources: Vec::new(),
        activity: None,
    });
    let mut discovered = git::DiscoveredRepo {
        path: directory.path().to_path_buf(),
        trees: vec![git::DiscoveredTree { path: directory.path().to_path_buf(), error: None }],
    };
    coordinator.apply_discovery(&common, Ok(discovered.clone()));
    assert_eq!(coordinator.passes.changed.len(), 1);
    coordinator.passes.changed.clear();
    coordinator.apply_discovery(&common, Ok(discovered.clone()));
    assert!(coordinator.passes.changed.is_empty());
    let new_path = directory.path().join("new-worktree");
    discovered.trees.push(git::DiscoveredTree { path: new_path.clone(), error: None });
    coordinator.apply_discovery(&common, Ok(discovered.clone()));
    assert_eq!(coordinator.passes.changed, HashSet::from([WorktreeKey::new(&common, &new_path)]));
    coordinator.passes.changed.clear();
    coordinator.apply_discovery(&common, Ok(discovered));
    assert!(coordinator.passes.changed.is_empty());
}

#[test]
fn polling_time_only_counts_serial_work_inside_the_last_minute() {
    let now = Instant::now();
    let mut polling = PollingTime::default();
    polling.record(now - Duration::from_secs(70), now - Duration::from_secs(55));
    polling.record(now - Duration::from_secs(40), now - Duration::from_secs(35));
    assert_eq!(polling.total(now), Duration::from_secs(10));
}
