#[test]
fn absent_configuration_is_recovered_when_created() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.is_empty());
    fixture.service().add(&fixture.repo).unwrap();
    let elapsed = wait_for_within(&monitor, |update| update.snapshot.repos.len() == 1,
        Duration::from_secs(11));
    assert!(elapsed < Duration::from_secs(10), "configuration creation took {elapsed:?}");
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
