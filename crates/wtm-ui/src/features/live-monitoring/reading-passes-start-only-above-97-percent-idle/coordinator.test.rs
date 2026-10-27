use super::*;

#[test]
fn reading_passes_use_the_displayed_idle_threshold() {
    for (milliseconds, allowed) in [(1499, true), (1500, false), (2100, false)] {
        let (_directory, mut coordinator) = scan_fixture(1);
        let now = Instant::now();
        let work = Duration::from_millis(milliseconds);
        coordinator.polling_time.record(now - work, now);
        let caption = crate::dashboard::DisplayItem::Status {
            health: crate::dashboard::Health::Live, polling_time_last_minute: work,
        }.caption();
        assert_eq!(caption, if milliseconds == 1499 { "[idle: 98%]" }
            else if milliseconds == 1500 { "[idle: 97%]" } else { "[idle: 96%]" });
        coordinator.try_scan_pass(now);
        assert_eq!(coordinator.scan_generation > 0, allowed);
        assert_eq!(coordinator.repos.values().all(|repo| repo.discovered.is_some()), allowed);
        if !allowed {
            assert_eq!(coordinator.passes.next, Some(now + Duration::from_secs(5)));
            assert!(coordinator.published.is_empty());
        }
    }
}

#[test]
fn busy_reading_passes_recheck_every_five_seconds_without_losing_work() {
    let (_directory, mut coordinator) = scan_fixture(1);
    let now = Instant::now();
    coordinator.polling_time.record(now - Duration::from_secs(2), now);
    coordinator.launch_changed = true;
    let common = &coordinator.repo_order[0];
    let key = WorktreeKey::new(common, &coordinator.repos[common].path);
    coordinator.passes.changed.insert(key.clone());
    let discoveries = coordinator.discoveries.clone();
    for second in [0, 5, 10] {
        coordinator.try_scan_pass(now + Duration::from_secs(second));
        assert_eq!(coordinator.passes.next, Some(now + Duration::from_secs(second + 5)));
        assert_eq!(coordinator.discoveries, discoveries);
        assert!(coordinator.passes.changed.contains(&key));
        assert!(coordinator.launch_changed);
        assert_eq!(coordinator.scan_generation, 0);
        coordinator.try_scan_pass(now + Duration::from_secs(second + 1));
        assert_eq!(coordinator.passes.next, Some(now + Duration::from_secs(second + 5)));
    }
    coordinator.try_scan_pass(now + Duration::from_secs(60));
    assert!(coordinator.scan_generation > 0);
    assert!(coordinator.discoveries.is_empty());
    assert!(!coordinator.launch_changed);
    assert!(coordinator.passes.changed.is_empty());
}

#[test]
fn recovery_waits_until_the_existing_retry_deadline() {
    let (_directory, mut coordinator) = scan_fixture(1);
    let now = Instant::now();
    coordinator.polling_time.record(now - Duration::from_secs(59), now - Duration::from_millis(57500));
    coordinator.try_scan_pass(now);
    coordinator.try_scan_pass(now + Duration::from_secs(1));
    assert_eq!(coordinator.scan_generation, 0);
    assert_eq!(coordinator.passes.next, Some(now + Duration::from_secs(5)));
    coordinator.try_scan_pass(now + Duration::from_secs(5));
    assert!(coordinator.scan_generation > 0);
}

#[test]
fn fresh_reading_passes_start_immediately_and_keep_the_normal_cooldown() {
    let (_directory, mut coordinator) = scan_fixture(1);
    let before = Instant::now();
    coordinator.try_scan_pass(before);
    assert!(coordinator.scan_generation > 0);
    let next = coordinator.passes.next.unwrap();
    assert!(next >= before + Duration::from_secs(10));
    let generation = coordinator.scan_generation;
    coordinator.discoveries.extend(coordinator.repo_order.iter().cloned());
    coordinator.try_scan_pass(next - Duration::from_nanos(1));
    assert_eq!(coordinator.scan_generation, generation);
    assert_eq!(coordinator.passes.next, Some(next));
}

#[test]
fn deferred_reading_passes_remain_cancellable() {
    let (_directory, mut coordinator) = scan_fixture(1);
    let now = Instant::now();
    coordinator.polling_time.record(now - Duration::from_secs(2), now);
    let latest = coordinator.latest.clone();
    let cancel = coordinator.cancel.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        coordinator.run();
        sender.send(()).unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while latest.lock().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(latest.lock().unwrap().as_ref().unwrap().scan_activities.is_empty());
    cancel.cancel();
    receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    worker.join().unwrap();
}
