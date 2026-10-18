use super::*;

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
    assert_eq!(view.items[1].display.caption(), "/repo");
    assert_eq!(view.items[1].display.caption_with_refresh(true), "/repo\nRefreshing");
    assert!(matches!(view.items[2].display, DisplayItem::PendingWorktree { .. }));
    assert!(!view.items.iter().any(|item| matches!(item.display, DisplayItem::Notice { error: true, .. })));
}

#[test]
fn scan_projection_keeps_completed_counts_and_identity_as_pending_rows_complete() {
    let targets = pending_targets();
    let first = Snapshot { config_error: None, repos: vec![repo(vec![tree("/repo", Head::Branch("main".into()), 8, 3, 1)])] };
    let view = DashboardView::from_scan(&first, &targets, &Default::default(), false);
    assert_eq!(view.items[0].display.caption(), "repo");
    assert_eq!(view.items[1].display.caption(), "main\nS +8/-3 · binaries: 1");
    assert_eq!(view.items[1].display.caption_with_refresh(true), "main\nS +8/-3 · binaries: 1\nRefreshing");
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
    let visibility = wtm_ui::visibility::Visibility::persist_toggle(&path, &repository, &tree).unwrap();
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
