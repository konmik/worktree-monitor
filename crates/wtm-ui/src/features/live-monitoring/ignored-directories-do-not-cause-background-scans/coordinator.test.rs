use super::*;

fn receive_change(coordinator: &mut Coordinator, key: &WorktreeKey) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !coordinator.passes.changed.contains_key(key) && Instant::now() < deadline {
        coordinator.receive_watch();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(coordinator.passes.changed.contains_key(key));
}

#[test]
fn ignored_hardlink_modifications_do_not_scan_but_tracked_edits_do() {
    let (directory, mut coordinator) = scan_fixture(1);
    let tree = directory.path().join("repo-0");
    std::fs::write(tree.join(".gitignore"), b"local.properties\ntracked.properties\n").unwrap();
    std::fs::write(tree.join("local.properties"), b"same\n").unwrap();
    std::fs::write(tree.join("tracked.properties"), b"before\n").unwrap();
    crate::process::checked(Some(&tree), &crate::process::args(&["add", "-f", "tracked.properties"]), &Cancel::new()).unwrap();
    let external = directory.path().join("shared.properties");
    std::fs::hard_link(tree.join("local.properties"), &external).unwrap();
    coordinator.scan_pass();
    let key = coordinator.worktrees.keys().next().unwrap().clone();
    coordinator.receive_watch();
    coordinator.passes.changed.clear();
    let before = coordinator.scan_generation;
    let before_additions = coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts.source.additions;
    std::fs::write(tree.join("local.properties"), b"same\n").unwrap();
    assert_eq!(std::fs::read(&external).unwrap(), b"same\n");
    receive_change(&mut coordinator, &key);
    coordinator.scan_pass();
    assert_eq!(coordinator.scan_generation, before, "ignored hardlink write repeated the scan");
    assert!(coordinator.passes.changed.is_empty());
    std::fs::write(tree.join("tracked.properties"), b"before\nafter\n").unwrap();
    receive_change(&mut coordinator, &key);
    coordinator.scan_pass();
    assert!(coordinator.scan_generation > before);
    assert_eq!(coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts.source.additions, before_additions + 1);
}

#[test]
fn forced_changes_are_not_erased_by_ignored_file_modifications() {
    let (directory, mut coordinator) = scan_fixture(1);
    let tree = directory.path().join("repo-0");
    std::fs::write(tree.join(".gitignore"), b"local.properties\n").unwrap();
    std::fs::write(tree.join("local.properties"), b"ignored\n").unwrap();
    coordinator.scan_pass();
    let key = coordinator.worktrees.keys().next().unwrap().clone();
    coordinator.receive_watch();
    coordinator.passes.changed.clear();
    for force_first in [false, true] {
        let before = coordinator.scan_generation;
        let mut paths = watch::WatchBatch::default();
        paths.worktrees.insert(key.clone());
        paths.worktree_paths.insert(key.clone(), HashMap::from([(
            normalize(&tree.join("local.properties")), notify::EventKind::Modify(notify::event::ModifyKind::Any),
        )]));
        let mut forced = watch::WatchBatch::default();
        forced.worktrees.insert(key.clone());
        forced.force_worktrees.insert(key.clone());
        if force_first {
            coordinator.enqueue(forced);
            coordinator.enqueue(paths);
        } else {
            coordinator.enqueue(paths);
            coordinator.enqueue(forced);
        }
        coordinator.scan_pass();
        assert!(coordinator.scan_generation > before);
    }
}

#[test]
fn edits_during_unignore_planning_are_included_in_the_same_pass() {
    let (directory, mut coordinator) = scan_fixture(1);
    let tree = directory.path().join("repo-0");
    std::fs::create_dir(tree.join("ignored")).unwrap();
    std::fs::write(tree.join(".gitignore"), b"ignored/\n").unwrap();
    let visible = tree.join("ignored/visible.txt");
    std::fs::write(&visible, b"one\n").unwrap();
    coordinator.scan_pass();
    coordinator.receive_watch();
    coordinator.passes.changed.clear();
    std::fs::write(tree.join(".gitignore"), b"").unwrap();
    let key = coordinator.worktrees.keys().next().unwrap().clone();
    receive_change(&mut coordinator, &key);
    coordinator.watch.before_plan = Some(Box::new(move || {
        std::fs::write(&visible, b"one\ntwo\n").unwrap();
        std::thread::sleep(Duration::from_millis(100));
    }));
    coordinator.scan_pass();
    assert_eq!(coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0]
        .result.as_ref().unwrap().counts.source.additions, 2,
        "edit during ignore planning was lost");
}
