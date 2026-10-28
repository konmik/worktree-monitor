use super::*;

fn drain_events(coordinator: &mut Coordinator) {
    for _ in 0..20 {
        coordinator.receive_watch();
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn assert_ignored_lifecycle_does_not_repeat_scans(change: impl FnOnce(&Path)) {
    let (directory, mut coordinator) = scan_fixture(1);
    let tree = directory.path().join("repo-0");
    std::fs::write(tree.join(".gitignore"), b"local.properties\n*.tmp\n").unwrap();
    std::fs::write(tree.join("local.properties"), b"ignored\n").unwrap();
    std::fs::write(tree.join("existing.tmp"), b"ignored\n").unwrap();
    coordinator.scan_pass();
    drain_events(&mut coordinator);
    coordinator.passes.changed.clear();
    coordinator.discoveries.clear();
    let before = coordinator.scan_generation;
    let before_additions = coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0]
        .result.as_ref().unwrap().counts.source.additions;
    change(&tree);
    drain_events(&mut coordinator);
    coordinator.scan_pass();
    assert_eq!(coordinator.scan_generation, before, "ignored file lifecycle repeated scans");
    std::fs::write(tree.join("visible.txt"), b"visible\n").unwrap();
    drain_events(&mut coordinator);
    coordinator.scan_pass();
    assert!(coordinator.scan_generation > before);
    assert_eq!(coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0]
        .result.as_ref().unwrap().counts.source.additions, before_additions + 1);
}

#[test]
fn ignored_file_creation_does_not_repeat_scans() {
    assert_ignored_lifecycle_does_not_repeat_scans(|tree| {
        std::fs::File::create(tree.join("created.tmp")).unwrap();
    });
}

#[test]
fn ignored_file_removal_does_not_repeat_scans() {
    assert_ignored_lifecycle_does_not_repeat_scans(|tree| {
        std::fs::remove_file(tree.join("local.properties")).unwrap();
    });
}

#[test]
fn ignored_file_rename_does_not_repeat_scans() {
    assert_ignored_lifecycle_does_not_repeat_scans(|tree| {
        std::fs::rename(tree.join("existing.tmp"), tree.join("renamed.tmp")).unwrap();
    });
}

#[test]
fn visible_directory_removal_is_not_hidden_by_a_file_ignore_rule() {
    let (directory, mut coordinator) = scan_fixture(1);
    let tree = directory.path().join("repo-0");
    std::fs::write(tree.join(".gitignore"), b"/swapped\n!/swapped/\n").unwrap();
    std::fs::create_dir(tree.join("swapped")).unwrap();
    std::fs::write(tree.join("swapped/visible.txt"), b"visible\n").unwrap();
    coordinator.scan_pass();
    drain_events(&mut coordinator);
    coordinator.passes.changed.clear();
    let before = coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0]
        .result.as_ref().unwrap().counts.source.additions;
    std::fs::rename(tree.join("swapped"), directory.path().join("moved")).unwrap();
    drain_events(&mut coordinator);
    coordinator.scan_pass();
    let after = coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0]
        .result.as_ref().unwrap().counts.source.additions;
    assert_eq!(after + 1, before, "moving a visible directory left stale counts");
}

#[test]
fn ignored_directory_replaced_by_visible_file_updates_counts() {
    let (directory, mut coordinator) = scan_fixture(1);
    let tree = directory.path().join("repo-0");
    std::fs::write(tree.join(".gitignore"), b"/swapped/\n").unwrap();
    std::fs::create_dir(tree.join("swapped")).unwrap();
    std::fs::write(tree.join("swapped/noise.txt"), b"ignored\n").unwrap();
    coordinator.scan_pass();
    drain_events(&mut coordinator);
    coordinator.passes.changed.clear();
    let before = coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0]
        .result.as_ref().unwrap().counts.source.additions;
    std::fs::remove_dir_all(tree.join("swapped")).unwrap();
    std::fs::write(tree.join("swapped"), b"visible\n").unwrap();
    drain_events(&mut coordinator);
    coordinator.scan_pass();
    let after = coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0]
        .result.as_ref().unwrap().counts.source.additions;
    assert_eq!(after, before + 1, "replacing an ignored directory hid a visible file");
}

#[test]
fn directory_removed_during_initial_planning_does_not_leave_stale_counts() {
    let (directory, mut coordinator) = scan_fixture(1);
    let tree = directory.path().join("repo-0");
    std::fs::write(tree.join(".gitignore"), b"/swapped\n!/swapped/\n").unwrap();
    std::fs::create_dir(tree.join("swapped")).unwrap();
    std::fs::write(tree.join("swapped/visible.txt"), b"visible\n").unwrap();
    let moved = directory.path().join("moved");
    let removed = tree.join("swapped");
    coordinator.watch.before_plan = Some(Box::new(move || {
        std::fs::rename(&removed, &moved).unwrap();
    }));
    coordinator.scan_pass();
    coordinator.watch.before_plan = None;
    let before = coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0]
        .result.as_ref().unwrap().counts.source.additions;
    drain_events(&mut coordinator);
    coordinator.scan_pass();
    let after = coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0]
        .result.as_ref().unwrap().counts.source.additions;
    assert_eq!(after + 1, before, "initial planning lost a removed visible directory");
}
