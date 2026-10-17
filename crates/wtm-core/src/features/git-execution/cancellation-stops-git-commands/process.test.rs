#[test]
fn cancellation_reaps_git_without_waiting_for_inherited_descendant_pipes() {
    let cancel = crate::Cancel::new();
    let request = cancel.clone();
    let trigger = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        request.cancel();
    });
    let start = std::time::Instant::now();
    let result = crate::process::run(None, &crate::process::args(&[
        "-c", "alias.wtm-test=!sleep 2", "wtm-test",
    ]), &cancel);
    assert_eq!(result.err().unwrap().kind, ErrorKind::Cancelled);
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    trigger.join().unwrap();
}

#[test]
fn cancellation_detaches_a_blocked_stdin_writer() {
    let cancel = crate::Cancel::new();
    let request = cancel.clone();
    let trigger = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        request.cancel();
    });
    let input = vec![b'x'; 16 * 1024 * 1024];
    let start = std::time::Instant::now();
    let result = crate::process::run_with_input(None, &crate::process::args(&[
        "-c", "alias.wtm-test=!sleep 2", "wtm-test",
    ]), &input, &cancel);
    assert_eq!(result.err().unwrap().kind, ErrorKind::Cancelled);
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    trigger.join().unwrap();
}
