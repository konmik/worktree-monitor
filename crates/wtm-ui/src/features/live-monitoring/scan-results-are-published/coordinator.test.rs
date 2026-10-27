use super::*;

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
    while !coordinator.passes.changed.contains_key(&key) && Instant::now() < deadline {
        coordinator.receive_watch();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(coordinator.passes.changed.contains_key(&key));
    coordinator.scan_pass();
    assert_eq!(coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts.source.additions, 1);
}
