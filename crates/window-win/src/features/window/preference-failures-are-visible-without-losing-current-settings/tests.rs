use super::*;

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
