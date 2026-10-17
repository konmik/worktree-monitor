use crate::git::{category, numstat, worktrees, Category};
use crate::poll::Registrations;
use crate::{Counts, ErrorKind, Head, LineCounts, RepoEntry, Snapshot, WorktreeEntry, WorktreeStats};
use std::path::Path;

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

include!("../../src/features/change-counting/worktree-counts-include-branch-changes/unit.test.rs");

include!("../../src/features/live-monitoring/process.test.rs");
