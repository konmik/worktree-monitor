use super::*;

fn drain_events(coordinator: &mut Coordinator) {
    for _ in 0..20 {
        coordinator.receive_watch();
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn assert_metadata_does_not_repeat_scans(relative: &str) {
    let (directory, mut coordinator) = scan_fixture(1);
    coordinator.scan_pass();
    drain_events(&mut coordinator);
    coordinator.passes.changed.clear();
    coordinator.discoveries.clear();
    let before = coordinator.scan_generation;
    let tree = directory.path().join("repo-0");
    let metadata = tree.join(".git").join(relative);
    std::fs::create_dir_all(metadata.parent().unwrap()).unwrap();
    std::fs::write(&metadata, b"metadata unrelated to worktree counts\n").unwrap();
    drain_events(&mut coordinator);
    coordinator.scan_pass();
    assert_eq!(coordinator.scan_generation, before, "irrelevant Git metadata repeated scans: {relative}");
    std::fs::write(tree.join("visible.txt"), b"visible\n").unwrap();
    drain_events(&mut coordinator);
    coordinator.scan_pass();
    assert!(coordinator.scan_generation > before);
    assert_eq!(coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0]
        .result.as_ref().unwrap().counts.source.additions, 1);
}

#[test]
fn cursor_metadata_does_not_repeat_scans() {
    assert_metadata_does_not_repeat_scans("cursor/events.log");
}

#[test]
fn fetch_head_metadata_does_not_repeat_scans() {
    assert_metadata_does_not_repeat_scans("FETCH_HEAD");
}
