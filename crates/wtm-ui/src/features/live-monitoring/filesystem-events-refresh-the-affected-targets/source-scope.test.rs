use super::*;

#[test]
fn repository_excludes_do_not_rescan_other_repositories() {
    let (directory, mut coordinator) = scan_fixture(2);
    let tree = directory.path().join("repo-0");
    std::fs::write(tree.join("ignored.txt"), b"visible before exclude\n").unwrap();
    coordinator.scan_pass();
    for _ in 0..20 {
        coordinator.receive_watch();
        std::thread::sleep(Duration::from_millis(10));
    }
    coordinator.passes.changed.clear();
    coordinator.discoveries.clear();
    let other_common = coordinator.repo_order[1].clone();
    let other_key = coordinator.repos[&other_common].worktrees[0].clone();
    let before = coordinator.worktrees[&other_key].activity.as_ref().unwrap().generation;
    std::fs::write(tree.join(".git/info/exclude"), b"ignored.txt\n").unwrap();
    for _ in 0..20 {
        coordinator.receive_watch();
        std::thread::sleep(Duration::from_millis(10));
    }
    coordinator.scan_pass();
    assert_eq!(coordinator.worktrees[&other_key].activity.as_ref().unwrap().generation, before,
        "repository-local excludes rescanned an unrelated repository");
    assert_eq!(coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts.source.additions, 0);
}
#[test]
fn repository_attributes_under_another_ignored_tree_keep_their_scope() {
    let (directory, mut coordinator) = scan_fixture(2);
    let tree = directory.path().join("repo-0");
    let other = directory.path().join("repo-1");
    std::fs::write(tree.join("tracked.txt"), b"source\n").unwrap();
    std::fs::write(other.join(".gitignore"), b"ignored/\n").unwrap();
    std::fs::create_dir(other.join("ignored")).unwrap();
    let attributes = other.join("ignored/attributes");
    std::fs::write(&attributes, b"").unwrap();
    crate::process::checked(Some(&tree), &crate::process::args(&[
        "config", "core.attributesFile", attributes.to_str().unwrap(),
    ]), &Cancel::new()).unwrap();
    coordinator.scan_pass();
    for _ in 0..20 {
        coordinator.receive_watch();
        std::thread::sleep(Duration::from_millis(10));
    }
    coordinator.passes.changed.clear();
    coordinator.discoveries.clear();
    let other_common = coordinator.repo_order[1].clone();
    let other_key = coordinator.repos[&other_common].worktrees[0].clone();
    let before = coordinator.worktrees[&other_key].activity.as_ref().unwrap().generation;
    std::fs::write(&attributes, b"tracked.txt binary\n").unwrap();
    for _ in 0..20 {
        coordinator.receive_watch();
        std::thread::sleep(Duration::from_millis(10));
    }
    coordinator.scan_pass();
    assert_eq!(coordinator.worktrees[&other_key].activity.as_ref().unwrap().generation, before);
    let counts = coordinator.snapshot.repos[0].worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts;
    assert_eq!(counts.source.additions, 0);
    assert_eq!(counts.binaries, 1);
}


#[test]
fn discovered_global_source_retries_a_failed_repository() {
    let (directory, mut coordinator) = scan_fixture(2);
    coordinator.scan_pass();
    let first = coordinator.repo_order[0].clone();
    let failed = coordinator.repo_order[1].clone();
    let source = directory.path().join("custom-global-config");
    std::fs::write(&source, b"before\n").unwrap();
    coordinator.repos.get_mut(&first).unwrap().sources.global.push(source.clone());
    coordinator.watch_dirty = true;
    coordinator.reconcile_watches();
    for _ in 0..20 {
        coordinator.receive_watch();
        std::thread::sleep(Duration::from_millis(10));
    }
    coordinator.passes.changed.clear();
    coordinator.discoveries.clear();
    let before = coordinator.repos[&failed].activity.as_ref().unwrap().generation;
    coordinator.repos.get_mut(&failed).unwrap().discovered = Some(Err(Error::config("repository unavailable")));
    std::fs::write(&source, b"after\n").unwrap();
    for _ in 0..20 {
        coordinator.receive_watch();
        std::thread::sleep(Duration::from_millis(10));
    }
    coordinator.scan_pass();
    assert!(coordinator.repos[&failed].activity.as_ref().unwrap().generation > before);
    assert!(coordinator.repos[&failed].discovered.as_ref().unwrap().is_ok());
}
