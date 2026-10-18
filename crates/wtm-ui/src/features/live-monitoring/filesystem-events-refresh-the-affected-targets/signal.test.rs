#[test]
fn cancellation_wait_wakes_for_non_cancellation_signals() {
    let cancel = crate::Cancel::new();
    let request = cancel.clone();
    let trigger = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        request.wake();
    });
    let start = std::time::Instant::now();
    cancel.wait(std::time::Duration::from_secs(5));
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    assert!(!cancel.is_cancelled());
    trigger.join().unwrap();
}
