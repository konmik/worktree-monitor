#[test]
fn subprocess_drains_both_pipes_and_preserves_exit_errors() {
    let output = crate::process::run(None, &crate::process::args(&[
        "-c", "alias.wtm-test=!printf 'out'; printf 'err' >&2; exit 7", "wtm-test",
    ]), &crate::Cancel::new()).unwrap();
    assert_eq!(output.stdout, b"out");
    assert_eq!(output.stderr, b"err");
    assert_eq!(output.code, 7);
    assert_eq!(crate::process::success(output, &[0]).unwrap_err().message, "err");
}

#[test]
fn subprocess_timeout_does_not_join_descendant_pipe_readers() {
    let start = std::time::Instant::now();
    let result = crate::process::run(None, &crate::process::args(&[
        "-c", "alias.wtm-test=!sleep 12", "wtm-test",
    ]), &crate::Cancel::new());
    assert_eq!(result.err().unwrap().kind, ErrorKind::Timeout);
    assert!(start.elapsed() < std::time::Duration::from_secs(11));
}

#[test]
fn large_stdout_and_stderr_are_drained_without_deadlock() {
    let output = crate::process::run(None, &crate::process::args(&[
        "-c", "alias.wtm-test=!head -c 131072 /dev/zero; head -c 131072 /dev/zero >&2", "wtm-test",
    ]), &crate::Cancel::new()).unwrap();
    assert_eq!(output.stdout, vec![0; 131072]);
    assert_eq!(output.stderr, vec![0; 131072]);
    assert_eq!(output.code, 0);
}

#[test]
fn subprocess_output_is_bounded() {
    let result = crate::process::run(None, &crate::process::args(&[
        "-c", "alias.wtm-test=!head -c 16777218 /dev/zero", "wtm-test",
    ]), &crate::Cancel::new());
    assert_eq!(result.err().unwrap().kind, ErrorKind::OutputLimit);
}
