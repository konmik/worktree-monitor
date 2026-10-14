use std::path::PathBuf;
use std::time::Duration;
use window_win::dashboard::{DashboardView, DisplayItem, Health, ItemKey};
use window_win::order::identity;
use wts_core::{Counts, Error, ErrorKind, Head, LineCounts, RepoEntry, Snapshot, WorktreeEntry, WorktreeStats};

fn tree(path: &str, head: Head, additions: u64, deletions: u64, binaries: u64) -> WorktreeEntry {
    WorktreeEntry { path: path.into(), result: Ok(WorktreeStats { head,
        counts: Counts { source: LineCounts { additions, deletions }, binaries, ..Counts::default() } }) }
}

fn repo(trees: Vec<WorktreeEntry>) -> RepoEntry {
    RepoEntry { common_dir: "/repo/.git".into(), path: "/repo".into(), worktrees: Ok(trees) }
}

fn failure() -> Error { Error { kind: ErrorKind::Git, message: "Cannot inspect worktree".into() } }

#[test]
fn keeps_per_row_counts_without_aggregation() {
    let snapshot = Snapshot { config_error: None, repos: vec![repo(vec![
        tree("/repo", Head::Branch("main".into()), u64::MAX, 9, 2),
        tree("/second", Head::Branch("topic".into()), 7, 3, 1),
    ])] };
    let view = DashboardView::from_snapshot(&snapshot);
    assert_eq!(view.items.len(), 3);
    assert_eq!(view.items[0].display.caption(), "repo · 2 worktrees");
    assert!(view.items[1].display.caption().contains("S +18446744073709551615/-9 · T +0/-0 · X +0/-0 · binaries: 2"));
    assert!(view.items[2].display.caption().contains("S +7/-3 · T +0/-0 · X +0/-0 · binaries: 1"));
    assert_eq!(view.health, Health::Live);
    assert_eq!(view.items[1].key, ItemKey::Worktree(identity(&PathBuf::from("/repo/.git")), identity(&PathBuf::from("/repo"))));
}

#[test]
fn renders_source_test_and_extra_counts_in_the_indicator() {
    let snapshot = Snapshot { config_error: None, repos: vec![repo(vec![WorktreeEntry {
        path: "/repo".into(),
        result: Ok(WorktreeStats { head: Head::Branch("main".into()), counts: Counts {
            source: LineCounts { additions: 1, deletions: 2 },
            test: LineCounts { additions: 3, deletions: 4 },
            extra: LineCounts { additions: 5, deletions: 6 },
            binaries: 7,
        } }),
    }])] };
    let view = DashboardView::from_snapshot(&snapshot);
    assert_eq!(view.items[1].display.caption(), "main\n/repo\nS +1/-2 · T +3/-4 · X +5/-6 · binaries: 7");
}

#[test]
fn partial_errors_preserve_known_counts_and_missing_values() {
    let mut entries = vec![tree("/repo", Head::Branch("main".into()), 4, 2, 1)];
    entries.push(WorktreeEntry { path: "/broken".into(), result: Err(failure()) });
    let snapshot = Snapshot { config_error: Some(Error { kind: ErrorKind::Config, message: "Invalid config".into() }),
        repos: vec![repo(entries), RepoEntry { common_dir: "/lost/.git".into(), path: "/lost".into(), worktrees: Err(failure()) }] };
    let view = DashboardView::from_snapshot(&snapshot);
    assert_eq!(view.items.len(), 6);
    assert!(view.items[2].display.caption().contains("S +4/-2 · T +0/-0 · X +0/-0 · binaries: 1"));
    assert_eq!(view.health.caption(Duration::from_secs(20)), "ERROR · Polling 20.0s in last minute");
    assert_eq!(view.items[0].display.caption(), "Configuration error\nConfig: Invalid config");
    assert_eq!(view.items[3].display.caption(), "/broken\nCounts unavailable · binaries unavailable\nERROR (Git): Cannot inspect worktree");
    assert_eq!(view.items[4].display.caption(), "lost · ? worktrees");
}

#[test]
fn status_formats_polling_time_for_the_last_minute() {
    let status = DisplayItem::Status { health: Health::Live, polling_time_last_minute: Duration::from_millis(1200) };
    assert_eq!(status.caption(), "LIVE · Polling 1.2s in last minute");
}

#[test]
fn branch_detached_and_unborn_are_distinct() {
    let snapshot = Snapshot { config_error: None, repos: vec![repo(vec![
        tree("/main", Head::Branch("main".into()), 1, 2, 0),
        tree("/detached", Head::Detached("abc123".into()), 0, 0, 0),
        tree("/new", Head::Unborn("next".into()), 0, 0, 0),
    ])] };
    let view = DashboardView::from_snapshot(&snapshot);
    let heads: Vec<_> = view.items.iter().skip(1).map(|item| item.display.caption().lines().next().unwrap().to_owned()).collect();
    assert_eq!(heads, ["main", "detached abc123", "unborn next"]);
}

#[test]
fn empty_bare_scanning_and_zero_measurements_are_truthful() {
    let empty = DashboardView::from_snapshot(&Snapshot::default());
    assert_eq!(empty.items[0].display.caption(), "No registered repositories.\n");
    assert_eq!(empty.health.caption(Duration::from_secs(5)), "LIVE · Polling 5.0s in last minute");
    let scanning = DashboardView::scanning();
    assert_eq!(scanning.health, Health::Scanning);
    assert_eq!(scanning.items.len(), 1);
    let bare = DashboardView::from_snapshot(&Snapshot { repos: vec![repo(vec![])], config_error: None });
    assert_eq!(bare.items[0].display.caption(), "repo · 0 worktrees\nNo working trees (bare repository).");
    let rows = DashboardView::from_snapshot(&Snapshot { config_error: None, repos: vec![repo(vec![
        tree("/zero", Head::Branch("main".into()), 0, 0, 0),
        tree("/binary", Head::Branch("main".into()), 0, 0, 2),
    ])] });
    assert_eq!(rows.items[1].display.caption(), "main\n/zero\nS +0/-0 · T +0/-0 · X +0/-0");
    assert_eq!(rows.items[2].display.caption(), "main\n/binary\nS +0/-0 · T +0/-0 · X +0/-0 · binaries: 2");
    assert!(matches!(rows.items[2].display, DisplayItem::Worktree { .. }));
}

#[test]
fn count_only_changes_keep_item_keys_and_row_failure_keeps_discovery_complete() {
    let first = Snapshot { repos: vec![repo(vec![tree("/repo", Head::Branch("main".into()), 1, 0, 0)])], config_error: None };
    let mut second = first.clone();
    second.repos[0].worktrees = Ok(vec![tree("/repo", Head::Branch("main".into()), 8, 3, 2)]);
    let before = DashboardView::from_snapshot(&first);
    let changed = DashboardView::from_snapshot(&second);
    assert_eq!(before.items.iter().map(|item| &item.key).collect::<Vec<_>>(), changed.items.iter().map(|item| &item.key).collect::<Vec<_>>());
    assert_eq!(changed.items[1].display.caption(), "main\n/repo\nS +8/-3 · T +0/-0 · X +0/-0 · binaries: 2");
    second.repos[0].worktrees.as_mut().unwrap()[0].result = Err(failure());
    let after = DashboardView::from_snapshot(&second);
    assert_eq!(before.items.iter().map(|item| &item.key).collect::<Vec<_>>(), after.items.iter().map(|item| &item.key).collect::<Vec<_>>());
    assert_eq!(after.items[0].display.caption(), "repo · 1 worktree");
    assert!(matches!(after.items[1].display, DisplayItem::Worktree { result: Err(_), .. }));
}

#[test]
fn repository_failure_does_not_invent_discovered_worktrees() {
    let snapshot = Snapshot { config_error: None, repos: vec![RepoEntry {
        common_dir: "/lost/.git".into(), path: "/lost".into(), worktrees: Err(failure()),
    }] };
    let view = DashboardView::from_snapshot(&snapshot);
    assert_eq!(view.health, Health::Errors);
    assert_eq!(view.items[1].key, ItemKey::RepositoryError(identity(&PathBuf::from("/lost/.git"))));
    assert_eq!(view.items[0].display.caption(), "lost · ? worktrees");
    assert_eq!(view.items[1].display.caption(), "Repository error · counts unavailable\n/lost\nGit: Cannot inspect worktree");
}

#[test]
fn repository_identity_ignores_display_path_and_preferences_have_a_distinct_notice() {
    let mut snapshot = Snapshot { config_error: None, repos: vec![repo(vec![])] };
    let before = DashboardView::from_snapshot(&snapshot);
    snapshot.repos[0].path = "/renamed".into();
    let mut after = DashboardView::from_snapshot(&snapshot);
    assert_eq!(before.items[0].key, after.items[0].key);
    after.presentation_warning("Locked");
    assert_eq!(after.health, Health::Errors);
    assert_eq!(after.items[0].key, ItemKey::Presentation);
    assert_eq!(after.items[0].display.caption(), "Presentation order warning\nLocked");
}

#[test]
fn launch_discovery_error_has_its_own_notice_and_error_health() {
    let snapshot = Snapshot::default();
    let mut view = DashboardView::from_snapshot(&snapshot);
    view.launch_error(&Error { kind: ErrorKind::Io, message: "Could not start Git".into() });
    assert_eq!(view.health.caption(Duration::from_secs(5)), "ERROR · Polling 5.0s in last minute");
    assert_eq!(view.items.len(), 2);
    assert_eq!(view.items[0].key, ItemKey::Launch);
    assert_eq!(view.items[0].display, DisplayItem::Notice {
        title: "Launch repository discovery error".into(), message: "Io: Could not start Git".into(), error: true,
    });
    assert_eq!(view.items[1].key, ItemKey::Empty);
    assert_eq!(DashboardView::from_snapshot(&snapshot).health, Health::Live);
}

#[test]
fn launch_discovery_error_preserves_rows_and_other_error_notices() {
    let snapshot = Snapshot { config_error: Some(Error { kind: ErrorKind::Config, message: "Invalid config".into() }),
        repos: vec![repo(vec![tree("/repo", Head::Branch("main".into()), 4, 2, 0)])] };
    let mut view = DashboardView::from_snapshot(&snapshot);
    let rows = view.items.clone();
    view.launch_error(&failure());
    view.presentation_warning("Locked");
    assert_eq!(view.health, Health::Errors);
    assert_eq!(view.items[0].key, ItemKey::Presentation);
    assert_eq!(view.items[1].key, ItemKey::Launch);
    assert_eq!(&view.items[2..], rows.as_slice());
}

#[test]
fn visibility_save_warning_preserves_rows_and_reports_error_health() {
    let snapshot = Snapshot { config_error: None,
        repos: vec![repo(vec![tree("/repo", Head::Branch("main".into()), 4, 2, 0)])] };
    let mut view = DashboardView::from_snapshot(&snapshot);
    let rows = view.items.clone();
    view.visibility_warning("Cannot lock worktree visibility");
    assert_eq!(view.health, Health::Errors);
    assert_eq!(view.items[0].key, ItemKey::Visibility);
    assert_eq!(view.items[0].display, DisplayItem::Notice {
        title: "Worktree visibility warning".into(), message: "Cannot lock worktree visibility".into(), error: true,
    });
    assert_eq!(&view.items[1..], rows.as_slice());
}
