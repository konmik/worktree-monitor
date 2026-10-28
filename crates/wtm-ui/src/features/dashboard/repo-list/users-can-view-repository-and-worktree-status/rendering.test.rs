use super::*;
use crate::dashboard::{ItemKey, ViewItem};
use crate::{Counts, Error, ErrorKind, Head, LineCounts, WorktreeStats};

fn line(buffer: &RenderBuffer, row: usize) -> String {
    buffer.row(row).unwrap().iter().map(|cell| cell.text()).collect::<String>().trim_end().to_owned()
}

#[test]
fn repository_and_path_names_reserve_refresh_and_action_columns() {
    let item = DisplayItem::Repository { name: "repository".into(), bare: false };
    let buffer = render_row(&item, 20, RowOptions { refreshing: true, heading_columns: Some(7), ..Default::default() });
    assert_eq!(line(&buffer, 0), "repo\u{2026} *");
    assert_eq!(buffer.row(0).unwrap()[6].color, TEAL);
    let path = render_name("C:/work/branch", 9, true, true, MUTED);
    assert_eq!(line(&path, 0), "\u{2026}branch *");
    let unicode = render_name("\u{e9}\u{1f642}abc", 4, false, false, WHITE);
    assert_eq!(line(&unicode, 0), "\u{e9}\u{1f642}\u{2026}");
    for columns in [0, 1, 2] {
        let buffer = render_row(&item, columns, RowOptions { refreshing: true, ..Default::default() });
        assert_eq!(buffer.cells().len(), columns);
    }
}

#[test]
fn counts_preserve_text_colors_and_mark_only_nonzero_line_groups_stressed() {
    let counts = Counts { source: LineCounts { additions: 3, deletions: 4 },
        test: LineCounts { additions: 5, deletions: 0 }, binaries: 2, ..Default::default() };
    let buffer = render_counts(&counts, 80, false);
    assert_eq!(line(&buffer, 0), counts.to_string());
    assert_eq!(buffer.row(0).unwrap()[2].color, GREEN);
    assert_eq!(buffer.row(0).unwrap()[5].color, RED);
    let mut x = 0;
    for part in crate::count_indicators::count_parts(&counts) {
        for cell in &buffer.row(0).unwrap()[x..x + display_width(&part.text)] {
            assert_eq!(cell.flags.stressed, part.opaque);
            assert!(!cell.flags.bold && !cell.flags.underline);
        }
        x += display_width(&part.text);
    }
    let blank = render_counts(&Counts { binaries: 2, ..Default::default() }, 80, false);
    assert!(blank.cells().iter().all(|cell| !cell.flags.stressed));
}

#[test]
fn narrow_errors_wrap_and_pending_rows_reserve_count_space() {
    let item = DisplayItem::Worktree { path: "C:/work/main".into(), result: Err(Error { kind: ErrorKind::Git, message: "a long failure message".into() }) };
    let wide = render_row(&item, 80, Default::default());
    let narrow = render_row(&item, 10, Default::default());
    assert!(narrow.rows() > wide.rows());
    assert_eq!(line(&narrow, 2), "ERROR");
    assert!(narrow.row(2).unwrap().iter().filter(|cell| cell.text() != " ").all(|cell| cell.color == RED));
    for columns in [0, 1, 5] {
        let buffer = render_row(&item, columns, Default::default());
        assert_eq!(buffer.cells().len(), buffer.rows() * columns);
    }
    let pending = render_row(&DisplayItem::PendingWorktree { path: "main".into() }, 20, Default::default());
    assert_eq!(pending.rows(), 2);
    assert!(line(&pending, 1).is_empty());
}

#[test]
fn dashboard_can_be_dumped_with_status_repository_branch_and_counts() {
    let view = DashboardView { health: Health::Live, items: vec![
        ViewItem { key: ItemKey::Repository("repo".into()), display: DisplayItem::Repository { name: "repo".into(), bare: false } },
        ViewItem { key: ItemKey::Worktree("repo".into(), "main".into()), display: DisplayItem::Worktree { path: "main".into(), result: Ok(WorktreeStats { pending_commits: 0, head: Head::Branch("main".into()), counts: Counts::default() }) } },
    ] };
    let buffer = render_dashboard(&view, crate::IdlePercentage::default(), 20, 8, |_| Default::default());
    assert!(line(&buffer, 0).contains("idle"));
    assert_eq!(buffer.row(0).unwrap()[0].color, GREEN);
    assert_eq!(line(&buffer, 2), "repo");
    assert_eq!(line(&buffer, 4), "main");
    assert_eq!(line(&buffer, 5), "S +0/-0");
    let dump = (0..buffer.rows()).map(|row| line(&buffer, row)).collect::<Vec<_>>().join("\n");
    assert!(dump.contains("repo\n\nmain\nS +0/-0"));
    let clipped = render_dashboard(&view, crate::IdlePercentage::default(), 4, 3, |_| Default::default());
    assert_eq!(clipped.cells().len(), 12);
    assert_eq!(line(&clipped, 2), "repo");
}

#[test]
fn refresh_markers_follow_display_columns_of_complete_graphemes() {
    for (name, width) in [("\u{754c}\u{1f642}", 4), ("e\u{301}\u{1f469}\u{200d}\u{1f4bb}", 3)] {
        let buffer = render_name(name, width + 2, false, true, WHITE);
        assert_eq!(line(&buffer, 0), format!("{name} *"));
        assert_eq!(buffer.row(0).unwrap()[width].text(), " ");
        assert_eq!(buffer.row(0).unwrap()[width + 1].text(), "*");
        assert_eq!(buffer.row(0).unwrap()[width + 1].color, TEAL);
    }
}

#[test]
fn scrolling_projects_complete_rows_below_a_fixed_header() {
    let header = text_row("header", 8, GREEN);
    let mut item = RenderBuffer::new(8, 4, MUTED);
    for row in 0..4 { item.write(0, row, &format!("line{row}"), WHITE); }
    let items = [item];
    for scroll in 0..=2 {
        let frame = compose_dashboard(&header, &items, &[], 8, 4, scroll);
        assert_eq!(frame.scroll_row, scroll);
        assert_eq!(line(&frame.buffer, 0), "header");
        assert_eq!(line(&frame.buffer, 1), "");
        assert_eq!(line(&frame.buffer, 2), format!("line{scroll}"));
        assert_eq!(line(&frame.buffer, 3), format!("line{}", scroll + 1));
        assert_eq!(frame.buffer.row(0), header.row(0));
    }
}

#[test]
fn scrolling_clamps_at_the_bottom_and_preserves_attached_row_positions() {
    let header = text_row("header", 8, GREEN);
    let items = [text_row("repo", 8, TEAL), text_row("branch", 8, WHITE), text_row("next", 8, TEAL)];
    let frame = compose_dashboard(&header, &items, &[true, false], 8, 4, usize::MAX);
    assert_eq!(frame.content_rows, 4);
    assert_eq!(frame.row_offsets, [0, 1, 3]);
    assert_eq!(frame.row_heights, [1, 1, 1]);
    assert_eq!(frame.scroll_row, 2);
    assert_eq!(line(&frame.buffer, 2), "");
    assert_eq!(line(&frame.buffer, 3), "next");
    let spacious = compose_dashboard(&header, &items, &[true, false], 8, 10, usize::MAX);
    assert_eq!(spacious.scroll_row, 0);
    assert_eq!(line(&spacious.buffer, 2), "repo");
    assert_eq!(line(&spacious.buffer, 3), "branch");
    assert_eq!(line(&spacious.buffer, 4), "");
    assert_eq!(line(&spacious.buffer, 5), "next");
}

#[test]
fn tiny_frames_and_empty_content_stay_within_grid_dimensions() {
    let header = text_row("header", 8, GREEN);
    let items = [text_row("body", 8, WHITE)];
    for columns in 0..=2 {
        for rows in 0..=3 {
            let frame = compose_dashboard(&header, &items, &[], columns, rows, usize::MAX);
            assert_eq!(frame.buffer.columns(), columns);
            assert_eq!(frame.buffer.rows(), rows);
            assert_eq!(frame.buffer.cells().len(), columns * rows);
            if rows > 1 { assert_eq!(line(&frame.buffer, 1), ""); }
        }
    }
    let empty = compose_dashboard(&header, &[], &[], 8, 4, usize::MAX);
    assert_eq!(empty.content_rows, 0);
    assert_eq!(empty.scroll_row, 0);
    assert!(empty.row_offsets.is_empty());
    assert!(empty.row_heights.is_empty());
    assert_eq!(line(&empty.buffer, 2), "");
}

#[test]
fn scroll_projection_keeps_wide_glyphs_whole_and_preserves_cell_flags() {
    let header = text_row("head", 4, GREEN);
    let mut item = RenderBuffer::new(4, 2, MUTED);
    item.write(0, 0, "skip", WHITE);
    item.write_styled(0, 1, "a\u{754c}b", RED, crate::render_buffer::CellFlags { stressed: true, bold: true, underline: true });
    let frame = compose_dashboard(&header, &[item.clone()], &[], 2, 3, 1);
    assert_eq!(line(&frame.buffer, 2), "a");
    assert_eq!(frame.buffer.row(2).unwrap()[0], item.row(1).unwrap()[0]);
    assert_eq!(frame.buffer.row(2).unwrap()[1].text(), " ");
    let whole = compose_dashboard(&header, &[item.clone()], &[], 3, 3, 1);
    assert_eq!(line(&whole.buffer, 2), "a\u{754c}");
    assert_eq!(whole.buffer.row(2).unwrap(), &item.row(1).unwrap()[..3]);
}

#[test]
fn scrollbar_drag_matches_the_visible_thumb_at_each_end() {
    let top = super::Scrollbar::new(100, 10, 0);
    assert_eq!(top.thumb, 1..2);
    assert_eq!(top.position_at(1, 0), 0);
    assert_eq!(top.position_at(8, 0), 90);
    assert_eq!(top.position_at(-20, 0), 0);
    assert_eq!(top.position_at(100, 0), 90);
    let bottom = super::Scrollbar::new(100, 10, 90);
    assert_eq!(bottom.thumb, 8..9);
    let short = super::Scrollbar::new(100, 2, 0);
    assert_eq!(short.position_at(10, 0), 0);
}
