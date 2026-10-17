

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

#[test]
fn writes_during_initial_scan_do_not_lose_later_edits() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.write(".gitignore", "ignored/\n");
    fixture.commit();
    let ignored = fixture.repo.join("ignored");
    fs::create_dir_all(&ignored).unwrap();
    fixture.service().add(&fixture.repo).unwrap();
    let start = Arc::new(Barrier::new(2));
    let writer_start = start.clone();
    let writer_directory = ignored.clone();
    let stop_writing = Cancel::new();
    let writer_cancel = stop_writing.clone();
    let writer = thread::spawn(move || {
        writer_start.wait();
        for index in 0..3000 {
            if writer_cancel.is_cancelled() { break; }
            let artifact = writer_directory.join(format!("active-{index}.txt"));
            fs::write(&artifact, format!("active {index}\n")).unwrap();
            fs::write(&artifact, format!("rewritten {index}\n")).unwrap();
            thread::sleep(Duration::from_millis(10));
        }
    });
    let mut monitor = fixture.service().monitor().unwrap();
    start.wait();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == Counts::default())));
    let before = settled_polling_time(&monitor);
    thread::sleep(Duration::from_secs(1));
    let after = monitor.latest().unwrap().polling_time_last_minute;
    let writing = !writer.is_finished();
    stop_writing.cancel();
    writer.join().unwrap();
    assert!(writing, "artifact creation ended before convergence was checked");
    assert!(after <= before + Duration::from_millis(500), "active artifacts added {before:?} -> {after:?}");
    fixture.write("tracked.txt", "before\nafter\n");
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(1, 0, 0), Duration::from_secs(18));
    monitor.stop().unwrap();
}
