use wtm_dashboard::dashboard::DisplayItem;
use super::row_layout::attached_rows;

#[test]
fn worktrees_attach_to_repository_and_worktree_rows() {
    let repository = DisplayItem::Repository { name: "repository".into(), bare: false };
    let branch = DisplayItem::PendingWorktree { path: "branch".into() };
    let notice = DisplayItem::Notice { title: "error".into(), message: "failure".into(), error: true };
    assert!(attached_rows(&repository, &branch));
    assert!(attached_rows(&branch, &branch));
    assert!(!attached_rows(&repository, &notice));
    assert!(!attached_rows(&branch, &repository));
}
