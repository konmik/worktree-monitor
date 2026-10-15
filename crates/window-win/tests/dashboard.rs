use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use window_win::dashboard::{DashboardView, DisplayItem, Health, ItemKey, RefreshTracker};
use window_win::order::identity;
use wts_core::{Counts, Error, ErrorKind, Head, LineCounts, RepoEntry, ScanActivity, ScanTarget, Snapshot, WorktreeEntry, WorktreeStats};

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
    assert_eq!(view.items[0].display.caption(), "repo");
    assert_eq!(view.items[1].display.caption(), "main\nS +18446744073709551615/-9 · T +0/-0 · X +0/-0 · binaries: 2");
    assert_eq!(view.items[2].display.caption(), "topic\nS +7/-3 · T +0/-0 · X +0/-0 · binaries: 1");
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
    assert_eq!(view.items[1].display.caption(), "main\nS +1/-2 · T +3/-4 · X +5/-6 · binaries: 7");
}

#[test]
fn partial_errors_preserve_known_counts_and_missing_values() {
    let mut entries = vec![tree("/repo", Head::Branch("main".into()), 4, 2, 1)];
    entries.push(WorktreeEntry { path: "/broken".into(), result: Err(failure()) });
    let snapshot = Snapshot { config_error: Some(Error { kind: ErrorKind::Config, message: "Invalid config".into() }),
        repos: vec![repo(entries), RepoEntry { common_dir: "/lost/.git".into(), path: "/lost".into(), worktrees: Err(failure()) }] };
    let view = DashboardView::from_snapshot(&snapshot);
    assert_eq!(view.items.len(), 6);
    assert_eq!(view.items[2].display.caption(), "main\nS +4/-2 · T +0/-0 · X +0/-0 · binaries: 1");
    assert_eq!(view.health.caption(Duration::from_secs(20)), "ERROR · Polling 20.0s in last minute");
    assert_eq!(view.items[0].display.caption(), "Configuration error\nConfig: Invalid config");
    assert_eq!(view.items[3].display.caption(), "/broken\nCounts unavailable · binaries unavailable\nERROR (Git): Cannot inspect worktree");
    assert_eq!(view.items[4].display.caption(), "lost");
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
    assert_eq!(bare.items[0].display.caption(), "repo\nNo working trees (bare repository).");
    let rows = DashboardView::from_snapshot(&Snapshot { config_error: None, repos: vec![repo(vec![
        tree("/zero", Head::Branch("main".into()), 0, 0, 0),
        tree("/binary", Head::Branch("main".into()), 0, 0, 2),
    ])] });
    assert_eq!(rows.items[1].display.caption(), "main\nS +0/-0 · T +0/-0 · X +0/-0");
    assert_eq!(rows.items[2].display.caption(), "main\nS +0/-0 · T +0/-0 · X +0/-0 · binaries: 2");
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
    assert_eq!(changed.items[1].display.caption(), "main\nS +8/-3 · T +0/-0 · X +0/-0 · binaries: 2");
    second.repos[0].worktrees.as_mut().unwrap()[0].result = Err(failure());
    let after = DashboardView::from_snapshot(&second);
    assert_eq!(before.items.iter().map(|item| &item.key).collect::<Vec<_>>(), after.items.iter().map(|item| &item.key).collect::<Vec<_>>());
    assert_eq!(after.items[0].display.caption(), "repo");
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
    assert_eq!(view.items[0].display.caption(), "lost");
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

fn scan(path: &str, generation: u64, started_at: Instant, finished_at: Option<Instant>) -> ScanActivity {
    ScanActivity { target: ScanTarget::Worktree { repository: "/repo/.git".into(), path: path.into() },
        generation, started_at, finished_at }
}

fn refreshing_tree(path: &str) -> ItemKey {
    ItemKey::Worktree(identity(&PathBuf::from("/repo/.git")), identity(&PathBuf::from(path)))
}

#[test]
fn refresh_completed_before_first_observation_lasts_one_second_and_expires_without_snapshot_change() {
    let start = Instant::now();
    let observed = start + Duration::from_secs(10);
    let activities = [scan("/repo", 1, start, Some(start + Duration::from_millis(1)))];
    let mut tracker = RefreshTracker::default();
    assert_eq!(tracker.update(&activities, observed), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, observed + Duration::from_millis(999)), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, observed + Duration::from_secs(1)), HashSet::new());
    assert_eq!(tracker.update(&activities, observed + Duration::from_secs(20)), HashSet::new());
}

#[test]
fn refresh_long_running_activity_stays_visible_until_finished_without_restarting() {
    let start = Instant::now();
    let mut activities = [scan("/repo", 1, start, None)];
    let mut tracker = RefreshTracker::default();
    assert_eq!(tracker.update(&activities, start), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(5)), HashSet::from([refreshing_tree("/repo")]));
    activities[0].finished_at = Some(start + Duration::from_secs(6));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(6)), HashSet::new());
}

#[test]
fn refresh_completion_within_minimum_visibility_does_not_restart_deadline() {
    let start = Instant::now();
    let mut activities = [scan("/repo", 1, start, None)];
    let mut tracker = RefreshTracker::default();
    assert_eq!(tracker.update(&activities, start), HashSet::from([refreshing_tree("/repo")]));
    activities[0].finished_at = Some(start + Duration::from_millis(500));
    assert_eq!(tracker.update(&activities, start + Duration::from_millis(500)), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(1)), HashSet::new());
}

#[test]
fn refresh_sibling_generations_have_independent_deadlines() {
    let start = Instant::now();
    let mut activities = [scan("/repo", 1, start, Some(start)), scan("/sibling", 2, start, Some(start))];
    let mut tracker = RefreshTracker::default();
    assert_eq!(tracker.update(&activities, start), HashSet::from([refreshing_tree("/repo"), refreshing_tree("/sibling")]));
    activities[1].generation = 3;
    assert_eq!(tracker.update(&activities, start + Duration::from_millis(500)), HashSet::from([refreshing_tree("/repo"), refreshing_tree("/sibling")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(1)), HashSet::from([refreshing_tree("/sibling")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_millis(1500)), HashSet::new());
    activities[0].generation = 4;
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(2)), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(3)), HashSet::new());
}

#[test]
fn repository_and_worktree_refreshes_have_independent_visibility() {
    let start = Instant::now();
    let repository = ItemKey::Repository(identity(&PathBuf::from("/repo/.git")));
    let mut activities = [
        ScanActivity { target: ScanTarget::Repository { common_dir: "/repo/.git".into(), path: "/display".into() },
            generation: 1, started_at: start, finished_at: Some(start) },
        scan("/repo", 2, start, Some(start)),
        scan("/sibling", 3, start, Some(start)),
    ];
    let mut tracker = RefreshTracker::default();
    assert_eq!(tracker.update(&activities, start), HashSet::from([repository.clone(), refreshing_tree("/repo"), refreshing_tree("/sibling")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(1)), HashSet::new());
    activities[1].generation = 4;
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(2)), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_millis(2999)), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(3)), HashSet::new());
    activities[0].generation = 5;
    activities[0].finished_at = None;
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(4)), HashSet::from([repository.clone()]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(6)), HashSet::from([repository]));
    activities[0].finished_at = Some(start + Duration::from_secs(7));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(7)), HashSet::new());
}

#[test]
fn refresh_removed_activity_is_pruned_and_reappearance_is_observed_again() {
    let start = Instant::now();
    let mut activities = [scan("/repo", 1, start, Some(start))];
    let mut tracker = RefreshTracker::default();
    assert_eq!(tracker.update(&activities, start), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&[], start + Duration::from_millis(100)), HashSet::new());
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(2)), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(3)), HashSet::new());
    assert_eq!(tracker.update(&[], start + Duration::from_secs(4)), HashSet::new());
    activities[0].generation = 2;
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(5)), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(6)), HashSet::new());
}

#[cfg(windows)]
#[test]
fn refresh_path_aliases_share_identity_and_do_not_restart_deadline() {
    let start = Instant::now();
    let mut activities = [
        ScanActivity { target: ScanTarget::Repository { common_dir: r"\\?\C:\Repo\.git".into(), path: r"C:\Display".into() },
            generation: 1, started_at: start, finished_at: Some(start) },
        ScanActivity { target: ScanTarget::Worktree { repository: r"\\?\C:\Repo\.git".into(), path: r"\\?\C:\Repo".into() },
            generation: 2, started_at: start, finished_at: Some(start) },
    ];
    let expected = HashSet::from([ItemKey::Repository("c:/repo/.git".into()), ItemKey::Worktree("c:/repo/.git".into(), "c:/repo".into())]);
    let mut tracker = RefreshTracker::default();
    assert_eq!(tracker.update(&activities, start), expected);
    activities[0].target = ScanTarget::Repository { common_dir: "c:/repo/.git".into(), path: "c:/other-display".into() };
    activities[1].target = ScanTarget::Worktree { repository: "c:/repo/.git".into(), path: "c:/repo".into() };
    assert_eq!(tracker.update(&activities, start + Duration::from_millis(500)), expected);
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(1)), HashSet::new());
}

fn pending_targets() -> Vec<ScanTarget> {
    vec![ScanTarget::Repository { common_dir: "/repo/.git".into(), path: "/repo".into() },
        ScanTarget::Worktree { repository: "/repo/.git".into(), path: "/repo".into() },
        ScanTarget::Worktree { repository: "/repo/.git".into(), path: "/second".into() }]
}

#[test]
fn pending_scan_projects_known_repository_and_discovered_paths_without_fake_counts_or_errors() {
    let view = DashboardView::from_scan(&Snapshot::default(), &pending_targets(), &Default::default(), false);
    assert_eq!(view.health, Health::Scanning);
    assert_eq!(view.items.len(), 3);
    assert_eq!(view.items[0].display.caption(), "repo");
    assert_eq!(view.items[1].key, refreshing_tree("/repo"));
    assert_eq!(view.items[1].display.caption(), "/repo\nBranch pending · counts pending · binaries pending");
    assert!(matches!(view.items[2].display, DisplayItem::PendingWorktree { .. }));
    assert!(!view.items.iter().any(|item| matches!(item.display, DisplayItem::Notice { error: true, .. })));
}

#[test]
fn scan_projection_keeps_completed_counts_and_identity_as_pending_rows_complete() {
    let targets = pending_targets();
    let first = Snapshot { config_error: None, repos: vec![repo(vec![tree("/repo", Head::Branch("main".into()), 8, 3, 1)])] };
    let view = DashboardView::from_scan(&first, &targets, &Default::default(), false);
    assert_eq!(view.items[0].display.caption(), "repo");
    assert_eq!(view.items[1].display.caption(), "main\nS +8/-3 · T +0/-0 · X +0/-0 · binaries: 1");
    assert_eq!(view.items[1].display.caption_with_refresh(true), "main\nS +8/-3 · T +0/-0 · X +0/-0 · binaries: 1\nRefreshing");
    assert_eq!(view.items[1].display.caption_with_refresh(false), view.items[1].display.caption());
    let second = Snapshot { config_error: None, repos: vec![repo(vec![tree("/repo", Head::Branch("main".into()), 8, 3, 1),
        tree("/second", Head::Branch("topic".into()), 2, 0, 0)])] };
    let completed = DashboardView::from_scan(&second, &targets, &Default::default(), false);
    assert_eq!(view.items.iter().map(|item| &item.key).collect::<Vec<_>>(), completed.items.iter().map(|item| &item.key).collect::<Vec<_>>());
    assert_eq!(completed.health, Health::Live);
}

#[test]
fn scan_projection_does_not_invent_trees_after_discovery_error_or_lose_notices() {
    let snapshot = Snapshot { config_error: Some(failure()), repos: vec![RepoEntry {
        common_dir: "/repo/.git".into(), path: "/repo".into(), worktrees: Err(failure()),
    }] };
    let expected = DashboardView::from_snapshot(&snapshot);
    let view = DashboardView::from_scan(&snapshot, &pending_targets(), &Default::default(), false);
    assert_eq!(view, expected);
}

#[test]
fn scan_projection_respects_hidden_pending_rows_and_show_all() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("visibility.json");
    let repository = directory.path().join("repo/.git");
    let tree = directory.path().join("repo");
    let visibility = window_win::visibility::Visibility::persist_toggle(&path, &repository, &tree).unwrap();
    let targets = vec![ScanTarget::Repository { common_dir: repository.clone(), path: tree.clone() },
        ScanTarget::Worktree { repository, path: tree }];
    let hidden = DashboardView::from_scan(&Snapshot::default(), &targets, &visibility, false);
    assert_eq!(hidden.items.len(), 1);
    assert_eq!(hidden.items[0].display.caption(), "repo");
    assert_eq!(hidden.health, Health::Scanning);
    let shown = DashboardView::from_scan(&Snapshot::default(), &targets, &visibility, true);
    assert_eq!(shown.items.len(), 2);
    assert!(matches!(shown.items[1].display, DisplayItem::PendingWorktree { .. }));
    let ScanTarget::Repository { common_dir, path } = &targets[0] else { unreachable!() };
    let snapshot = Snapshot { config_error: None, repos: vec![RepoEntry {
        common_dir: common_dir.clone(), path: path.clone(), worktrees: Ok(vec![WorktreeEntry { path: path.clone(), result: Err(failure()) }]),
    }] };
    let hidden_error = DashboardView::from_scan(&snapshot, &targets, &visibility, false);
    assert_eq!(hidden_error.items.len(), 1);
    assert_eq!(hidden_error.items[0].display.caption(), "repo");
    assert_eq!(hidden_error.health, Health::Errors);
    assert!(matches!(DashboardView::from_scan(&snapshot, &targets, &visibility, true).items[1].display,
        DisplayItem::Worktree { result: Err(_), .. }));
}

#[test]
fn scan_targets_control_membership_when_snapshot_is_stale_or_targets_grow() {
    let snapshot = Snapshot { config_error: None, repos: vec![
        repo(vec![tree("/repo", Head::Branch("main".into()), 1, 0, 0), WorktreeEntry { path: "/second".into(), result: Err(failure()) }]),
        RepoEntry { common_dir: "/stale/.git".into(), path: "/stale".into(), worktrees: Err(failure()) },
    ] };
    let mut targets = pending_targets();
    targets.pop();
    let view = DashboardView::from_scan(&snapshot, &targets, &Default::default(), false);
    assert_eq!(view.health, Health::Live);
    assert_eq!(view.items.len(), 2);
    assert_eq!(view.items[0].display.caption(), "repo");
    targets.push(ScanTarget::Worktree { repository: "/repo/.git".into(), path: "/new".into() });
    let grown = DashboardView::from_scan(&snapshot, &targets, &Default::default(), false);
    assert_eq!(grown.health, Health::Scanning);
    assert_eq!(grown.items.len(), 3);
    assert_eq!(grown.items[0].display.caption(), "repo");
    assert_eq!(grown.items[2].key, refreshing_tree("/new"));
    assert!(matches!(grown.items[2].display, DisplayItem::PendingWorktree { .. }));
    assert_eq!(DashboardView::from_scan(&snapshot, &[], &Default::default(), false),
        DashboardView::from_snapshot(&Snapshot::default()));
}

#[test]
fn scan_projection_preserves_bare_unknown_and_notice_precedence() {
    let targets = vec![ScanTarget::Repository { common_dir: "/repo/.git".into(), path: "/repo".into() }];
    let unknown = DashboardView::from_scan(&Snapshot::default(), &targets, &Default::default(), false);
    assert_eq!(unknown.health, Health::Scanning);
    assert_eq!(unknown.items[0].display.caption(), "repo");
    let snapshot = Snapshot { config_error: None, repos: vec![repo(vec![])] };
    assert_eq!(DashboardView::from_scan(&snapshot, &targets, &Default::default(), false), DashboardView::from_snapshot(&snapshot));
    let snapshot = Snapshot { config_error: Some(failure()), repos: vec![repo(vec![])] };
    let mut view = DashboardView::from_scan(&snapshot, &targets, &Default::default(), false);
    view.launch_error(&failure());
    view.presentation_warning("Order unavailable");
    view.visibility_warning("Visibility unavailable");
    assert_eq!(view.health, Health::Errors);
    assert_eq!(view.items.into_iter().map(|item| item.key).collect::<Vec<_>>(), vec![
        ItemKey::Visibility, ItemKey::Presentation, ItemKey::Launch, ItemKey::Configuration,
        ItemKey::Repository(identity(&PathBuf::from("/repo/.git"))),
    ]);
}

#[cfg(windows)]
#[test]
fn scan_projection_matches_completed_path_aliases() {
    let targets = vec![ScanTarget::Repository { common_dir: r"\\?\C:\Repo\.git".into(), path: r"C:\Repo".into() },
        ScanTarget::Worktree { repository: r"C:\REPO\.git".into(), path: r"\\?\C:\Repo".into() }];
    let snapshot = Snapshot { config_error: None, repos: vec![RepoEntry {
        common_dir: "c:/repo/.git".into(), path: "c:/repo".into(),
        worktrees: Ok(vec![tree("c:/repo", Head::Branch("main".into()), 1, 0, 0)]),
    }] };
    let pending = DashboardView::from_scan(&Snapshot::default(), &targets, &Default::default(), false);
    let completed = DashboardView::from_scan(&snapshot, &targets, &Default::default(), false);
    assert_eq!(pending.items.into_iter().map(|item| item.key).collect::<Vec<_>>(),
        completed.items.iter().map(|item| item.key.clone()).collect::<Vec<_>>());
    assert_eq!(completed.health, Health::Live);
    assert!(matches!(completed.items[1].display, DisplayItem::Worktree { result: Ok(_), .. }));
}
