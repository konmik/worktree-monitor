#[test]
fn worktree_edits_wait_for_the_next_serial_pass() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    fixture.register(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == Counts::default())));
    fixture.write("tracked.txt", "before\nafter\n");
    let elapsed = wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(1, 0, 0), MONITOR_UPDATE_TIMEOUT);
    assert!(elapsed >= Duration::from_secs(9), "edit bypassed the pass delay: {elapsed:?}");
    monitor.stop().unwrap();
}

#[test]
fn continuous_edits_do_not_scan_immediately_or_postpone_scheduled_updates() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    fixture.register(&fixture.repo).unwrap();
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
    let deadline = Instant::now() + MONITOR_UPDATE_TIMEOUT;
    loop {
        fixture.write("tracked.txt", "before\nafter\n");
        if monitor.latest().unwrap().snapshot.repos[0].worktrees.as_ref().unwrap()[0]
            .result.as_ref().is_ok_and(|stats| stats.counts == source_counts(1, 0, 0)) { break; }
        assert!(Instant::now() < deadline, "continuous edits postponed the scheduled update");
        thread::sleep(Duration::from_millis(100));
    }
    monitor.stop().unwrap();
}
