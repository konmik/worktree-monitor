use crate::dashboard::DisplayItem;
use crate::rendering::{render_row, RowOptions, TEAL};
use crate::{Counts, Head, WorktreeStats};

fn worktree(name: &str, pending_commits: u64) -> DisplayItem {
    DisplayItem::Worktree { path: "repo".into(), result: Ok(WorktreeStats {
        head: Head::Branch(name.into()), counts: Counts::default(), pending_commits,
    }) }
}

#[test]
fn commit_indicator_is_right_aligned_and_accessible() {
    let item = worktree("feature", 3);
    let row = render_row(&item, 20, RowOptions::default());
    let text: String = row.row(0).unwrap().iter().map(|cell| cell.text()).collect();
    assert_eq!(text, "feature         [^3]");
    for cell in &row.row(0).unwrap()[16..] {
        assert_eq!(cell.color, TEAL);
        assert!(cell.flags.stressed);
    }
    assert_eq!(item.caption(), "feature [^3]\nS +0/-0");
    let zero = render_row(&worktree("feature", 0), 20, RowOptions::default());
    assert!(zero.cells().iter().all(|cell| cell.text() != "["));
}

#[test]
fn commit_indicator_reserves_refresh_and_control_columns() {
    let row = render_row(&worktree("very-long-feature", 123), 25,
        RowOptions { refreshing: true, heading_columns: Some(18), ..Default::default() });
    let cells = row.row(0).unwrap();
    assert_eq!(cells[10].text(), "*");
    let text: String = cells[12..18].iter().map(|cell| cell.text()).collect();
    assert_eq!(text, "[^123]");
    assert!(cells[18..].iter().all(|cell| cell.text() == " "));
    for columns in 0..4 {
        let row = render_row(&worktree("feature", 3), columns, RowOptions::default());
        assert!(row.cells().iter().all(|cell| cell.text() != "[" && cell.text() != "]"));
    }
    let row = render_row(&worktree("feature", 3), 4, RowOptions::default());
    assert_eq!(row.row(0).unwrap().iter().map(|cell| cell.text()).collect::<String>(), "[^3]");
}

#[test]
fn commit_indicator_keeps_wide_branch_characters_whole() {
    let row = render_row(&worktree("\u{754c}\u{754c}\u{754c}", 3), 10, RowOptions::default());
    assert_eq!(row.row(0).unwrap()[6].text(), "[");
    assert_eq!(row.row(0).unwrap()[5].text(), " ");
    assert_eq!(row.columns(), 10);
}
