use super::*;

#[test]
fn dashboard_fits_at_multiple_sizes_and_dpis() {
    for dpi in [96, 120, 144, 192] {
        for (width, height) in [(900, 700), (420, 250), (180, 110), (20, 20)] {
            let layout = dashboard(width, height, dpi, Typography::new(dpi, 15 * dpi / 96), 7 * dpi / 96, 112 * dpi / 96).unwrap();
            for rect in [layout.status, layout.toggle, layout.viewport, layout.scrollbar] {
                assert!(rect.left >= 0 && rect.top >= 0, "{rect:?}");
                assert!(rect.left + rect.width <= width, "{rect:?}");
                assert!(rect.top + rect.height <= height, "{rect:?}");
            }
            assert_eq!(layout.status.left + layout.status.width, layout.toggle.left);
            assert!(layout.viewport.top >= layout.toggle.top + layout.toggle.height);
            assert!(layout.viewport.height > 0);
        }
    }
}

#[test]
fn scrollbar_has_a_gutter_when_compact_padding_is_smaller_than_a_character() {
    for dpi in [96, 120, 144, 192] {
        for height in [90, 130, 250] {
            let layout = dashboard(600, height, dpi, Typography::new(dpi, 15 * dpi / 96), 11 * dpi / 96, 0).unwrap();
            assert!(layout.scrollbar.left >= layout.viewport.left + layout.viewport.width);
            assert!(layout.scrollbar.width >= 1);
            assert!(layout.scrollbar.left + layout.scrollbar.width <= 600);
        }
    }
}

#[test]
fn dashboard_reduces_side_padding_and_preserves_top_padding() {
    for (dpi, cell, space, left, top, right) in [(96, 15, 7, 4, 3, 11), (120, 18, 9, 4, 4, 13),
        (144, 22, 11, 5, 5, 16), (192, 30, 14, 8, 6, 22)] {
        let layout = dashboard(900, 700, dpi, Typography::new(dpi, cell), space, 0).unwrap();
        assert_eq!(layout.status.left, left);
        assert_eq!(layout.status.top, top);
        assert_eq!(layout.toggle.left + layout.toggle.width, 900 - right);
        assert_eq!(layout.viewport.left, left);
        assert_eq!(layout.viewport.left + layout.viewport.width, 900 - right);
        assert_eq!(layout.viewport.top + layout.viewport.height, 700);
        assert!(layout.scrollbar.left >= 900 - right);
    }
    for (width, height) in [(1, 1), (20, 20), (180, 110), (420, 250)] {
        let layout = dashboard(width, height, 96, Typography::new(96, 15), 40, 0).unwrap();
        assert_eq!(layout.status.left, 0);
        assert_eq!(layout.status.top, 0);
        assert_eq!(layout.viewport.left, 0);
        assert!(layout.scrollbar.left >= 0);
        assert!(layout.scrollbar.left + layout.scrollbar.width <= width);
    }
}

#[test]
fn rows_stack_and_actions_stay_inside_measured_rows() {
    for dpi in [96, 120, 144, 192] {
        for width in [1, 20, 120, 800] {
            let spacing = RowSpacing::new(Typography::new(dpi, 15 * dpi / 96), false, false);
            let sizes = [RowSize { height: 40, action_width: None, spacing }, RowSize { height: 64, action_width: Some(78), spacing }, RowSize { height: 220, action_width: Some(78), spacing }];
            let layout = content(width, dpi, Typography::new(dpi, 15 * dpi / 96), &sizes).unwrap();
            let mut bottom = 0;
            for (index, row) in layout.rows.iter().enumerate() {
                assert_eq!(row.rect.top, bottom + if index == 0 { 0 } else { 8 * dpi / 96 });
                assert_eq!(row.rect.width, width);
                assert_eq!(row.rect.height, sizes[index].height);
                if let Some(action) = row.action {
                    assert!(action.left >= 0 && action.top >= 0);
                    assert!(action.left + action.width <= row.rect.width);
                    assert!(action.top + action.height <= row.rect.height);
                }
                bottom = row.rect.top + row.rect.height;
            }
            assert_eq!(layout.height, bottom);
        }
    }
    assert_eq!(content(800, 96, Typography::new(96, 15), &[]).unwrap().height, 0);
}

#[test]
fn first_branch_attaches_to_heading_without_changing_other_gaps() {
    for (dpi, cell, boundary) in [(96, 15, 15), (120, 18, 18), (144, 22, 22), (192, 30, 30), (96, 12, 14)] {
        let typography = Typography::new(dpi, cell);
        let heading = RowSpacing::new(typography, false, true);
        let branch = RowSpacing::new(typography, true, false);
        let separate = RowSpacing::new(typography, false, false);
        for heading_lines in [1, 2, 3] {
            let sizes = [
                RowSize { height: heading.height(typography, heading_lines), action_width: None, spacing: heading },
                RowSize { height: branch.height(typography, 3), action_width: Some(78), spacing: branch },
                RowSize { height: separate.height(typography, 2), action_width: Some(78), spacing: separate },
                RowSize { height: separate.height(typography, 1), action_width: None, spacing: separate },
            ];
            let layout = content(800, dpi, typography, &sizes).unwrap();
            let rows = &layout.rows;
            let heading_origin = rows[0].rect.top + heading.top(typography, heading_lines - 1);
            let branch_origin = rows[1].rect.top + branch.top(typography, 0);
            assert_eq!(branch_origin - heading_origin, boundary);
            assert_eq!(rows[1].rect.top, rows[0].rect.top + rows[0].rect.height);
            assert_eq!(rows[1].action.unwrap().top, branch.top(typography, 0));
            for index in [2, 3] {
                assert_eq!(rows[index].rect.top - rows[index - 1].rect.top - rows[index - 1].rect.height, 8 * dpi / 96);
            }
            assert_eq!(rows[2].action.unwrap().top, separate.top(typography, 0));
            assert_eq!(rows[1].rect.height - branch.top(typography, 2), cell + 8 * dpi / 96);
        }
    }
}

#[test]
fn adjacent_worktrees_follow_the_last_text_line_without_padding() {
    for (dpi, cell) in [(96, 15), (120, 18), (144, 22), (192, 30)] {
        let typography = Typography::new(dpi, cell);
        let attached = RowSpacing::new(typography, true, true);
        let last = RowSpacing::new(typography, true, false);
        for lines in [1, 2, 7] {
            for width in [180, 380, 800] {
                let sizes = [
                    RowSize { height: attached.height(typography, lines), action_width: Some(78), spacing: attached },
                    RowSize { height: last.height(typography, 2), action_width: Some(78), spacing: last },
                ];
                let layout = content(width, dpi, typography, &sizes).unwrap();
                let previous = &layout.rows[0];
                let next = &layout.rows[1];
                let last_line = previous.rect.top + attached.top(typography, lines - 1);
                assert_eq!(next.rect.top - last_line, typography.advance.max(cell));
                assert_eq!(next.rect.top, previous.rect.top + previous.rect.height);
                assert_eq!(next.action.unwrap().top, 0);
                assert_eq!(next.rect.height, typography.advance + cell + typography.padding);
            }
        }
    }
}

#[test]
fn hidden_controls_release_width_without_changing_vertical_geometry() {
    for dpi in [96, 120, 144, 192] {
        for width in [180, 380, 800] {
            let typography = Typography::new(dpi, 15 * dpi / 96);
            let hidden = dashboard(width, 540, dpi, typography, 7 * dpi / 96, 0).unwrap();
            let visible = dashboard(width, 540, dpi, typography, 7 * dpi / 96, 100).unwrap();
            assert_eq!(hidden.status.left, visible.status.left);
            assert!(hidden.status.width > visible.status.width);
            assert_eq!(hidden.toggle.width, 0);
            assert_eq!(hidden.viewport, visible.viewport);
            assert_eq!(hidden.status.top, visible.status.top);
            assert_eq!(hidden.status.height, visible.status.height);
            let spacing = RowSpacing::new(typography, true, true);
            let hidden = content(width, dpi, typography, &[RowSize { height: 60, action_width: None, spacing }]).unwrap();
            let visible = content(width, dpi, typography, &[RowSize { height: 60, action_width: Some(60), spacing }]).unwrap();
            assert_eq!(hidden.rows[0].rect, visible.rows[0].rect);
            assert!(hidden.rows[0].action.is_none());
            assert_eq!(visible.rows[0].action.unwrap().width, 60);
        }
    }
}
