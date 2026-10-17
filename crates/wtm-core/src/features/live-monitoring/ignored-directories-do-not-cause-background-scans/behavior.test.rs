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
fn replaced_refs_watch_observes_a_ref_only_head_and_count_change() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    git(&fixture.repo, &["switch", "--quiet", "-c", "alternate"]);
    fixture.write("tracked.txt", "alternate\n");
    fixture.commit();
    let alternate = String::from_utf8(git(&fixture.repo, &["rev-parse", "alternate"]))
        .unwrap().trim().to_owned();
    git(&fixture.repo, &["switch", "--quiet", "main"]);
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees| {
            trees[0].result.as_ref().is_ok_and(|stats| {
                stats.head == Head::Branch("main".into()) && stats.counts == Counts::default()
            })
        }));
    let refs = fixture.repo.join(".git/refs");
    let moved = fixture.repo.join(".git/refs-away");
    fs::rename(&refs, &moved).unwrap();
    fs::rename(&moved, &refs).unwrap();
    git(&fixture.repo, &["update-ref", "refs/heads/main", &alternate]);
    let common = dunce::canonicalize(fixture.repo.join(".git")).unwrap();
    wait_for_within(&monitor, |update| {
        let Some(repo) = update.snapshot.repos.first() else { return false; };
        let Some(trees) = repo.worktrees.as_ref().ok() else { return false; };
        repo.common_dir == common && trees.len() == 1
            && trees[0].result.as_ref().is_ok_and(|stats| {
                stats.head == Head::Branch("main".into())
                    && stats.counts == source_counts(1, 1, 0)
            })
    }, Duration::from_secs(18));
    monitor.stop().unwrap();
}

#[test]
fn replaced_info_directory_reconnects_exclude_watch() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    fixture.write("ignored.txt", "untracked\n");
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees| {
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == source_counts(1, 0, 0))
        }));
    let info = fixture.repo.join(".git/info");
    let moved = fixture.repo.join(".git/info-away");
    fs::rename(&info, &moved).unwrap();
    fs::create_dir(&info).unwrap();
    fs::write(info.join("exclude"), b"ignored.txt\n").unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts == Counts::default(),
        Duration::from_secs(18));
    fs::write(info.join("exclude"), b"").unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(1, 0, 0), Duration::from_secs(18));
    monitor.stop().unwrap();
    fs::remove_dir_all(moved).unwrap();
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
fn ignored_artifact_bursts_do_not_repeat_scans_and_real_edits_update() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.write(".gitignore", "ignored/\n");
    let ignored = fixture.repo.join("ignored");
    fs::create_dir_all(&ignored).unwrap();
    fixture.write("ignored/tracked.txt", "before\n");
    git(&fixture.repo, &["add", "-f", "ignored/tracked.txt"]);
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor_from(fixture.repo.clone()).unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == Counts::default()))
        && update.polling_time_last_minute > Duration::ZERO);
    settled_polling_time(&monitor);
    thread::sleep(Duration::from_secs(11));
    let before = monitor.latest().unwrap().polling_time_last_minute;
    let before_generations: Vec<_> = monitor.latest().unwrap().scan_activities.iter()
        .map(|activity| activity.generation).collect();
    for index in 0..100 {
        let artifact = ignored.join(format!("artifact-{index}.jsonl"));
        fs::write(&artifact, format!("ignored {index}\n")).unwrap();
        fs::write(&artifact, format!("rewritten {index}\n")).unwrap();
        if index % 5 == 4 { thread::sleep(Duration::from_millis(220)); }
    }
    thread::sleep(Duration::from_secs(12));
    let after = monitor.latest().unwrap().polling_time_last_minute;
    let after_generations: Vec<_> = monitor.latest().unwrap().scan_activities.iter()
        .map(|activity| activity.generation).collect();
    assert_eq!(after_generations, before_generations);
    assert!(after.saturating_sub(before) <= before,
        "ignored burst repeated more work than startup: {before:?} -> {after:?}");
    fixture.write("ignored/tracked.txt", "before\nafter\n");
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(1, 0, 0), Duration::from_secs(18));
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

#[test]
fn forced_tracked_files_under_ignored_directories_still_update() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.write(".gitignore", "ignored/\n");
    fs::create_dir_all(fixture.repo.join("ignored")).unwrap();
    fixture.write("ignored/tracked.txt", "before\n");
    git(&fixture.repo, &["add", "-f", "ignored/tracked.txt"]);
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == Counts::default())));
    fixture.write("ignored/tracked.txt", "before\nafter\n");
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(1, 0, 0), Duration::from_secs(18));
    monitor.stop().unwrap();
}

#[test]
fn ignore_rule_changes_are_observed_in_the_next_pass() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.write(".gitignore", "ignored/\n");
    fixture.commit();
    let ignored = fixture.repo.join("ignored");
    fs::create_dir_all(&ignored).unwrap();
    fs::write(ignored.join("visible-after-rule-change.txt"), "visible\n").unwrap();
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == Counts::default())));
    fs::write(fixture.repo.join(".gitignore"), b"").unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(1, 1, 0), Duration::from_secs(18));
    monitor.stop().unwrap();
}

#[test]
fn negated_files_under_an_ignored_listing_prefix_are_not_filtered() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.write(".gitignore", "dir/*\n!dir/visible.txt\n");
    fixture.commit();
    let directory = fixture.repo.join("dir");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("ignored.txt"), "ignored\n").unwrap();
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == Counts::default())));
    let visible = directory.join("visible.txt");
    fs::write(&visible, "one\n").unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(1, 0, 0), Duration::from_secs(18));
    fs::write(&visible, "one\ntwo\n").unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(2, 0, 0), Duration::from_secs(18));
    monitor.stop().unwrap();
}

#[test]
fn ignored_file_delete_recreate_and_directory_replacement_are_observed() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.write(".gitignore", "/swapped\n!/swapped/\n");
    fixture.commit();
    let swapped = fixture.repo.join("swapped");
    fs::write(&swapped, "ignored\n").unwrap();
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == Counts::default())));
    fs::remove_file(&swapped).unwrap();
    fs::create_dir(&swapped).unwrap();
    fs::write(swapped.join("visible.txt"), "visible\n").unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(1, 0, 0), Duration::from_secs(18));
    fs::remove_file(swapped.join("visible.txt")).unwrap();
    fs::remove_dir(&swapped).unwrap();
    fs::write(&swapped, "ignored again\n").unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts == Counts::default(),
        Duration::from_secs(18));
    monitor.stop().unwrap();
}

#[test]
fn ignored_directory_rename_and_delete_preserve_tracked_changes() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.write(".gitignore", "ignored*/\n");
    fs::create_dir_all(fixture.repo.join("ignored")).unwrap();
    fixture.write("ignored/tracked.txt", "before\n");
    git(&fixture.repo, &["add", "-f", "ignored/tracked.txt"]);
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == Counts::default())));
    let moved = fixture.repo.join("ignored-moved");
    fs::rename(fixture.repo.join("ignored"), &moved).unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(0, 1, 0), Duration::from_secs(18));
    fs::remove_file(moved.join("tracked.txt")).unwrap();
    fs::remove_dir(&moved).unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(0, 1, 0), Duration::from_secs(2));
    monitor.stop().unwrap();
}

#[test]
fn ignored_untracked_gitattributes_changes_update_counts() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.write(".gitignore", ".gitattributes\n");
    fixture.commit();
    fixture.write(".gitattributes", "");
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == Counts::default())));
    fixture.write("tracked.txt", "before\nafter\n");
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(1, 0, 0), Duration::from_secs(18));
    fixture.write(".gitattributes", "tracked.txt binary\n");
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(0, 0, 1), Duration::from_secs(18));
    monitor.stop().unwrap();
}

#[test]
fn global_excludes_changes_are_observed_in_the_next_pass() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    let excludes = fixture._directory.path().join("global-ignore");
    fs::write(&excludes, "globally-ignored.txt\n").unwrap();
    git(&fixture.repo, &["config", "core.excludesFile", excludes.to_str().unwrap()]);
    fixture.write("globally-ignored.txt", "ignored\n");
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == Counts::default())));
    fs::write(&excludes, b"").unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(1, 0, 0), Duration::from_secs(18));
    monitor.stop().unwrap();
}
