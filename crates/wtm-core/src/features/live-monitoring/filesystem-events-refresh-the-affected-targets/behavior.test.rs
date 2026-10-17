#[test]
fn untracked_directory_and_atomic_rename_changes_are_observed() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees| trees.len() == 1));
    let directory = fixture.repo.join("newdir");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("new.txt"), "one\ntwo\n").unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(2, 0, 0), Duration::from_secs(18));
    fs::rename(directory.join("new.txt"), directory.join("renamed.txt")).unwrap();
    fs::remove_file(directory.join("renamed.txt")).unwrap();
    let elapsed = wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == Counts::default(), Duration::from_secs(18));
    assert!(elapsed >= Duration::from_secs(9), "removal bypassed the pass delay: {elapsed:?}");
    monitor.stop().unwrap();
}

#[test]
fn metadata_changes_and_worktree_topology_are_observed() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    let linked = fixture._directory.path().join("linked");
    git(&fixture.repo, &["worktree", "add", "--quiet", "--detach", linked.to_str().unwrap()]);
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees| trees.len() == 2));
    fs::write(linked.join("linked.txt"), "linked\n").unwrap();
    git(&linked, &["add", "linked.txt"]);
    git(&linked, &["-c", "user.name=Wtm Test", "-c", "user.email=wtm@example.invalid",
        "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", "commit", "--quiet", "-m", "Linked"]);
    let linked_head = String::from_utf8(git(&linked, &["rev-parse", "HEAD"])).unwrap().trim().to_owned();
    let elapsed = wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap().iter().any(|tree|
            tree.result.as_ref().is_ok_and(|stats| stats.head == Head::Detached(linked_head.clone())
                && stats.counts == source_counts(1, 0, 0))),
        Duration::from_secs(18));
    assert!(elapsed < Duration::from_secs(18), "scheduled linked HEAD took {elapsed:?}");
    let added = fixture._directory.path().join("added");
    git(&fixture.repo, &["worktree", "add", "--quiet", "--detach", added.to_str().unwrap()]);
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap().len() == 3, Duration::from_secs(18));
    git(&fixture.repo, &["worktree", "remove", "--force", added.to_str().unwrap()]);
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap().len() == 2, Duration::from_secs(18));
    fixture.write("tracked.txt", "before\nmain\n");
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap().iter().any(|tree|
            tree.result.as_ref().is_ok_and(|stats| stats.head == Head::Branch("main".into())
                && stats.counts == source_counts(1, 0, 0))),
        Duration::from_secs(18));
    git(&fixture.repo, &["add", "tracked.txt"]);
    git(&fixture.repo, &["-c", "user.name=Wtm Test", "-c", "user.email=wtm@example.invalid",
        "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", "commit", "--quiet", "-m", "Main"]);
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap().iter().all(|tree|
            tree.result.as_ref().is_ok_and(|stats|
                stats.head == Head::Branch("main".into()) && stats.counts == Counts::default()
                    || stats.head == Head::Detached(linked_head.clone()) && stats.counts == source_counts(1, 0, 0))),
        Duration::from_secs(18));
    monitor.stop().unwrap();
}

#[test]
fn unchanged_worktrees_are_not_scanned_after_the_initial_pass() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == Counts::default()))
        && update.polling_time_last_minute > Duration::ZERO);
    thread::sleep(Duration::from_secs(1));
    let settled = monitor.latest().unwrap();
    let counts = settled.snapshot.repos[0].worktrees.as_ref().unwrap()[0]
        .result.as_ref().unwrap().counts;
    let polling_time = settled.polling_time_last_minute;
    thread::sleep(Duration::from_secs(12));
    let idle = monitor.latest().unwrap();
    assert_eq!(idle.snapshot.as_ref(), settled.snapshot.as_ref());
    assert_eq!(idle.snapshot.repos[0].worktrees.as_ref().unwrap()[0]
        .result.as_ref().unwrap().counts, counts);
    assert_eq!(idle.polling_time_last_minute, polling_time);
    monitor.stop().unwrap();
}

#[test]
fn git_object_writes_do_not_rescan_and_ref_changes_still_update() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    let objects = fixture.repo.join(".git/objects/e6");
    fs::create_dir_all(&objects).unwrap();
    let object = objects.join("tmp_obj_fixture");
    let bytes = b"irrelevant object data\n";
    fs::write(&object, bytes).unwrap();
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == Counts::default())));
    let before = settled_polling_time(&monitor);
    for _ in 0..10 {
        fs::write(&object, bytes).unwrap();
        thread::sleep(Duration::from_millis(200));
    }
    thread::sleep(Duration::from_millis(1200));
    assert!(monitor.latest().unwrap().polling_time_last_minute.saturating_sub(before) <= before);
    fixture.write("tracked.txt", "before\nafter\n");
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(1, 0, 0), Duration::from_secs(18));
    fixture.commit();
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts == Counts::default(),
        Duration::from_secs(18));
    monitor.stop().unwrap();
}
