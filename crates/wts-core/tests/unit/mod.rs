use crate::git::{numstat, worktrees};
use crate::poll::{Backoff, Registrations};
use crate::{Counts, ErrorKind, Snapshot};

#[test]
fn rename_unicode_and_binary_numstat() {
    let bytes = "2\t3\t\0旧 name.txt\0new\tname.txt\0-\t-\timage.bin\x000\t0\tunchanged.txt\0";
    assert_eq!(numstat(bytes.as_bytes()).unwrap(), Counts { additions: 2, deletions: 3, binaries: 1 });
}

#[test]
fn rejects_incomplete_rename_and_count_overflow() {
    assert_eq!(numstat(b"1\t0\t\0old\0").unwrap_err().kind, ErrorKind::InvalidOutput);
    assert_eq!(numstat(b"18446744073709551615\t0\ta\x001\t0\tb\0").unwrap_err().kind, ErrorKind::InvalidOutput);
    assert_eq!(numstat(b"1\t0\ta").unwrap_err().kind, ErrorKind::InvalidOutput);
}

#[test]
fn parses_nul_worktree_record_framing() {
    let bytes = "worktree C:/repo\0HEAD 123\0branch refs/heads/main\0locked reason\0\0worktree C:/missing 树\0HEAD 456\0detached\0prunable gitdir missing\0\0worktree C:/bare\0bare\0\0";
    let trees = worktrees(bytes.as_bytes()).unwrap();
    assert_eq!(trees.len(), 3);
    assert_eq!(trees[0].path.to_str().unwrap(), "C:/repo");
    assert_eq!(trees[1].prunable.as_deref(), Some("prunable gitdir missing"));
    assert!(trees[2].bare);
    assert_eq!(worktrees(b"worktree C:/repo\0HEAD 123\0").unwrap_err().kind, ErrorKind::InvalidOutput);
}

#[test]
fn unchanged_backoff_caps_and_changes_reset() {
    let mut backoff = Backoff::default();
    let snapshot = Snapshot::default();
    let observations: Vec<_> = (0..6).map(|_| backoff.observe(&snapshot)).collect();
    assert_eq!(observations, vec![5, 10, 20, 30, 30, 30]);
    let changed = Snapshot { config_error: Some(crate::Error::config("broken")), ..snapshot };
    assert_eq!(backoff.observe(&changed), 5);
    assert_eq!(backoff.observe(&changed), 10);
}

#[test]
fn malformed_reload_preserves_last_good_and_recovers() {
    let mut registrations = Registrations::default();
    let directory = std::env::current_dir().unwrap().join("fixture/.git");
    let bytes = serde_json::to_vec(&serde_json::json!({ "repos": [directory] })).unwrap();
    assert!(registrations.reload(Ok(bytes.clone())));
    assert_eq!(registrations.repos, vec![directory]);
    assert!(registrations.reload(Ok(b"malformed".to_vec())));
    assert_eq!(registrations.repos.len(), 1);
    assert_eq!(registrations.error.as_ref().unwrap().kind, ErrorKind::Config);
    assert!(registrations.reload(Ok(bytes)));
    assert_eq!(registrations.error, None);
    assert!(registrations.reload(Ok(Vec::new())));
    assert!(registrations.repos.is_empty());
}

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
        "-c", "alias.wts-test=!sleep 2", "wts-test",
    ]), &cancel);
    assert_eq!(result.err().unwrap().kind, ErrorKind::Cancelled);
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    trigger.join().unwrap();
}

#[test]
fn subprocess_drains_both_pipes_and_preserves_exit_errors() {
    let output = crate::process::run(None, &crate::process::args(&[
        "-c", "alias.wts-test=!printf 'out'; printf 'err' >&2; exit 7", "wts-test",
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
        "-c", "alias.wts-test=!sleep 12", "wts-test",
    ]), &crate::Cancel::new());
    assert_eq!(result.err().unwrap().kind, ErrorKind::Timeout);
    assert!(start.elapsed() < std::time::Duration::from_secs(11));
}

#[test]
fn large_stdout_and_stderr_are_drained_without_deadlock() {
    let output = crate::process::run(None, &crate::process::args(&[
        "-c", "alias.wts-test=!head -c 131072 /dev/zero; head -c 131072 /dev/zero >&2", "wts-test",
    ]), &crate::Cancel::new()).unwrap();
    assert_eq!(output.stdout, vec![0; 131072]);
    assert_eq!(output.stderr, vec![0; 131072]);
    assert_eq!(output.code, 0);
}

#[test]
fn subprocess_output_is_bounded() {
    let result = crate::process::run(None, &crate::process::args(&[
        "-c", "alias.wts-test=!head -c 16777218 /dev/zero", "wts-test",
    ]), &crate::Cancel::new());
    assert_eq!(result.err().unwrap().kind, ErrorKind::OutputLimit);
}
