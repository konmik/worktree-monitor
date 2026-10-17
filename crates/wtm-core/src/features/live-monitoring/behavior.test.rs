#[test]
fn monitoring_does_not_execute_fsmonitor_helpers() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    let hook = fixture.repo.join(".git/fsmonitor-hook");
    let marker = fixture.repo.join(".git/fsmonitor-called");
    fs::write(&hook, b"#!/bin/sh\nprintf 'called\\n' >> .git/fsmonitor-called\nprintf 'wtm-token\\0/\\0'\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(&fixture.repo, &["config", "core.fsmonitor", ".git/fsmonitor-hook"]);
    git(&fixture.repo, &["config", "core.fsmonitorHookVersion", "2"]);
    fixture.write("tracked.txt", "before\nafter\n");
    git(&fixture.repo, &["diff", "--numstat", "HEAD", "--"]);
    assert!(fs::read_to_string(&marker).unwrap().starts_with("called\n"));
    fs::remove_file(&marker).unwrap();
    assert_eq!(fixture.stats().counts, source_counts(1, 0, 0));
    assert!(!marker.exists());
}

#[test]
fn absent_configuration_is_recovered_when_created() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.is_empty());
    fixture.service().add(&fixture.repo).unwrap();
    let elapsed = wait_for_within(&monitor, |update| update.snapshot.repos.len() == 1,
        Duration::from_secs(6));
    assert!(elapsed < Duration::from_secs(5), "configuration creation took {elapsed:?}");
    monitor.stop().unwrap();
}

#[test]
fn removed_registered_root_recovers_after_recreation() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && !update.snapshot.has_errors());
    let moved = fixture.repo.with_file_name("moved");
    fs::rename(&fixture.repo, &moved).unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1 && update.snapshot.has_errors());
    fs::rename(&moved, &fixture.repo).unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos.len() == 1
        && !update.snapshot.has_errors(), Duration::from_secs(18));
    monitor.stop().unwrap();
}

#[test]
fn monitor_reloads_preserves_last_good_recovers_and_stops_promptly() {
    let fixture = Fixture::new("sha1");
    fixture.write("new.txt", "line\n");
    fixture.service().add(&fixture.repo).unwrap();
    let good = fs::read(&fixture.config).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1);
    fs::write(&fixture.config, b"broken").unwrap();
    wait_for(&monitor, |update| update.snapshot.config_error.is_some() && update.snapshot.repos.len() == 1);
    fs::write(&fixture.config, good).unwrap();
    wait_for(&monitor, |update| update.snapshot.config_error.is_none() && update.snapshot.repos.len() == 1);
    fixture.service().remove(&fixture.repo).unwrap();
    wait_for(&monitor, |update| update.snapshot.text() == "No registered repositories.\n");
    let start = Instant::now();
    monitor.stop().unwrap();
    assert!(start.elapsed() < Duration::from_secs(1));
}
