use super::*;

#[test]
fn polling_time_only_counts_serial_work_inside_the_last_minute() {
    let now = Instant::now();
    let polling = PollingTime::default();
    polling.record(now - Duration::from_secs(70), now - Duration::from_secs(55));
    polling.record(now - Duration::from_secs(40), now - Duration::from_secs(35));
    assert_eq!(polling.total(now), Duration::from_secs(10));
}

#[test]
fn idle_status_recalculates_while_waiting_without_worker_publication() {
    let (_directory, mut coordinator) = scan_fixture(0);
    let now = Instant::now();
    coordinator.polling_time.record(now - Duration::from_secs(61), now - Duration::from_secs(58));
    coordinator.publish_update();
    let cached = coordinator.latest.lock().unwrap().clone().unwrap();
    let monitor = Monitor {
        cancel: coordinator.cancel.clone(), latest: coordinator.latest.clone(),
        polling_time: coordinator.polling_time.clone(), worker: None,
    };
    let first = monitor.latest().unwrap();
    std::thread::sleep(Duration::from_millis(1250));
    let second = monitor.latest().unwrap();
    std::thread::sleep(Duration::from_millis(1250));
    let third = monitor.latest().unwrap();
    assert!(first.polling_time_last_minute > second.polling_time_last_minute);
    assert!(second.polling_time_last_minute > third.polling_time_last_minute);
    assert_eq!(third.polling_time_last_minute, Duration::ZERO);
    assert!(Arc::ptr_eq(&cached, coordinator.latest.lock().unwrap().as_ref().unwrap()));
    let caption = |update: &MonitorUpdate| crate::dashboard::DisplayItem::Status {
        health: crate::dashboard::Health::Live,
        polling_time_last_minute: update.polling_time_last_minute,
    }.caption();
    assert_ne!(caption(&first), caption(&second));
    assert_ne!(caption(&second), caption(&third));
}

#[test]
fn idle_time_is_available_before_the_first_snapshot() {
    let polling_time = PollingTime::default();
    let monitor = Monitor {
        cancel: Cancel::new(), latest: Arc::new(Mutex::new(None)),
        polling_time: polling_time.clone(), worker: None,
    };
    let _reading = polling_time.start();
    std::thread::sleep(Duration::from_millis(10));
    assert!(monitor.latest().is_none());
    assert!(monitor.polling_time_last_minute() >= Duration::from_millis(10));
}

#[test]
fn idle_status_changes_while_the_reading_worker_is_blocked() {
    let (_directory, mut coordinator) = scan_fixture(1);
    let latest = coordinator.latest.clone();
    let cancel = coordinator.cancel.clone();
    let polling_time = coordinator.polling_time.clone();
    let (blocked, wait_for_block) = std::sync::mpsc::channel();
    let (release, wait_for_release) = std::sync::mpsc::channel();
    coordinator.watch.before_plan = Some(Box::new(move || {
        blocked.send(()).unwrap();
        wait_for_release.recv().unwrap();
    }));
    let worker = std::thread::spawn(move || coordinator.scan_pass());
    let mut monitor = Monitor { cancel, latest, polling_time, worker: Some(worker) };
    wait_for_block.recv_timeout(Duration::from_secs(10)).unwrap();
    let first = monitor.latest().unwrap();
    std::thread::sleep(Duration::from_millis(1250));
    let second = monitor.latest().unwrap();
    std::thread::sleep(Duration::from_millis(1250));
    let third = monitor.latest().unwrap();
    release.send(()).unwrap();
    monitor.stop().unwrap();
    let stopped = monitor.latest().unwrap().polling_time_last_minute;
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(monitor.latest().unwrap().polling_time_last_minute, stopped);
    assert_eq!(first.scan_activities, third.scan_activities);
    assert!(second.polling_time_last_minute >= first.polling_time_last_minute + Duration::from_secs(1),
        "idle measurement froze while the reading worker was blocked");
    assert!(third.polling_time_last_minute >= second.polling_time_last_minute + Duration::from_secs(1));
    let caption = |update: &MonitorUpdate| crate::dashboard::DisplayItem::Status {
        health: crate::dashboard::Health::Live,
        polling_time_last_minute: update.polling_time_last_minute,
    }.caption();
    assert_ne!(caption(&first), caption(&second));
    assert_ne!(caption(&second), caption(&third));
}
