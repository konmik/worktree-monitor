#[test]
fn worktree_edits_wait_for_the_next_serial_pass() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == Counts::default())));
    fixture.write("tracked.txt", "before\nafter\n");
    let elapsed = wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(1, 0, 0), Duration::from_secs(18));
    assert!(elapsed >= Duration::from_secs(9), "edit bypassed the pass delay: {elapsed:?}");
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

#[test]
fn continuous_edits_do_not_scan_immediately_or_postpone_scheduled_updates() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == Counts::default())));
    let before = monitor.latest().unwrap().polling_time_last_minute;
    for _ in 0..10 {
        fixture.write("tracked.txt", "before\nafter\n");
        thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(monitor.latest().unwrap().polling_time_last_minute, before);
    let deadline = Instant::now() + Duration::from_secs(18);
    loop {
        fixture.write("tracked.txt", "before\nafter\n");
        if monitor.latest().unwrap().snapshot.repos[0].worktrees.as_ref().unwrap()[0]
            .result.as_ref().is_ok_and(|stats| stats.counts == source_counts(1, 0, 0)) { break; }
        assert!(Instant::now() < deadline, "continuous edits postponed the scheduled update");
        thread::sleep(Duration::from_millis(100));
    }
    monitor.stop().unwrap();
}
