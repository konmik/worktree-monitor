use crate::git::{category, numstat, worktrees, Category};
use crate::poll::{Registrations, WorktreeBackoff};
use crate::{Counts, ErrorKind, Head, LineCounts, RepoEntry, Snapshot, WorktreeEntry, WorktreeStats};
use std::path::Path;

#[test]
fn rename_unicode_and_binary_numstat() {
    let bytes = "2\t3\t\0旧 name.txt\0tests/new\tname.test.js\0-\t-\timage.bin\x000\t0\tunchanged.txt\0";
    assert_eq!(numstat(bytes.as_bytes()).unwrap(), Counts {
        test: LineCounts { additions: 2, deletions: 3 }, binaries: 1, ..Counts::default()
    });
}

#[test]
fn classifier_matches_plugin_rules_and_test_precedence() {
    for path in ["src/main.rs", "src/testing/foo.rs", "src/scripts.rs", "src/foo.test", "test.rs",
        "contest_foo.rs", "vitest.config", "vitest.configured.js", "src/foo.sh.txt", "script/build.rs"] {
        assert_eq!(category(Path::new(path)), Category::Source, "{path}");
    }
    for path in ["src\\Tests\\fixture.rs", "src/foo.test.ts", "src/foo.spec.ts", "src/foo_test.ts", "test-foo.rs",
        "test_foo.rs", "tests.config.js", "test.setup.ts", "tests.setup.ts", "tests/tools/build.ps1", "scripts/unit.spec.sh"] {
        assert_eq!(category(Path::new(path)), Category::Test, "{path}");
    }
    for directory in ["test", "tests", "__tests__", "e2e", "spec", "testdata", "__mocks__", "__fixtures__", "fixtures", "__snapshots__"] {
        assert_eq!(category(Path::new(&format!("scripts/{directory}/file.sh"))), Category::Test);
    }
    for prefix in ["ava", "cypress", "jest", "karma", "playwright", "vitest"] {
        assert_eq!(category(Path::new(&format!("{prefix}.config.ts"))), Category::Test);
    }
    for directory in ["tools", "scripts", "development_tools", "development-tools", "tooling"] {
        assert_eq!(category(Path::new(&format!("{directory}/build.rs"))), Category::Extra);
    }
    for extension in ["ps1", "sh", "bash", "bat", "cmd"] {
        assert_eq!(category(Path::new(&format!("src/build.{extension}"))), Category::Extra);
        assert_eq!(category(Path::new(&format!(".{extension}"))), Category::Extra);
    }
    assert_eq!(category(Path::new("src/build.PS1")), Category::Extra);
}

#[test]
fn snapshot_text_uses_the_categorized_indicator() {
    let snapshot = Snapshot { config_error: None, repos: vec![RepoEntry {
        common_dir: "/repo/.git".into(), path: "/repo".into(), worktrees: Ok(vec![WorktreeEntry {
            path: "/repo".into(), result: Ok(WorktreeStats { head: Head::Branch("main".into()), counts: Counts {
                source: LineCounts { additions: 1, deletions: 2 },
                test: LineCounts { additions: 3, deletions: 4 },
                extra: LineCounts { additions: 5, deletions: 6 }, binaries: 7,
            } }),
        }]),
    }] };
    assert!(snapshot.text().contains("S +1/-2 · T +3/-4 · X +5/-6 · binaries: 7"));
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
    let mut backoff = WorktreeBackoff::default();
    let result = Ok(WorktreeStats { head: Head::Branch("main".into()), counts: Counts::default() });
    let observations: Vec<_> = (0..6).map(|_| backoff.observe(&result).as_secs()).collect();
    assert_eq!(observations, vec![5, 10, 20, 30, 30, 30]);
    let changed = Err(crate::Error::new(ErrorKind::Git, "broken"));
    assert_eq!(backoff.observe(&changed).as_secs(), 5);
    assert_eq!(backoff.observe(&changed).as_secs(), 10);
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
