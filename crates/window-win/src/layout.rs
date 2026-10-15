use std::io;
use crate::typography::{RowSpacing, Typography};
use std::sync::{Mutex, MutexGuard};
use yoga_sys::*;
use yoga_sys::YGDirection::YGDirectionLTR;
use yoga_sys::YGEdge::{YGEdgeBottom, YGEdgeHorizontal, YGEdgeRight, YGEdgeTop};
use yoga_sys::YGFlexDirection::YGFlexDirectionRow;
use yoga_sys::YGPositionType::YGPositionTypeAbsolute;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Rect {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    fn offset(self, parent: Self) -> Self {
        Self { left: self.left + parent.left, top: self.top + parent.top, ..self }
    }
}

pub(crate) struct DashboardLayout {
    pub status: Rect,
    pub toggle: Rect,
    pub viewport: Rect,
    pub scrollbar: Rect,
}

pub(crate) struct RowSize {
    pub height: i32,
    pub action: Option<i32>,
    pub spacing: RowSpacing,
}

pub(crate) struct RowLayout {
    pub rect: Rect,
    pub action: Option<Rect>,
}

pub(crate) struct ContentLayout {
    pub rows: Vec<RowLayout>,
    pub height: i32,
}

static YOGA: Mutex<()> = Mutex::new(());

struct Tree {
    root: YGNodeRef,
    _guard: MutexGuard<'static, ()>,
}

impl Tree {
    fn new() -> io::Result<Self> {
        let guard = YOGA.lock().map_err(|_| io::Error::other("Yoga layout lock poisoned"))?;
        let node = unsafe { YGNodeNew() };
        if node.is_null() { Err(io::Error::other("Could not allocate Yoga layout")) } else { Ok(Self { root: node, _guard: guard }) }
    }

    unsafe fn child(&self, parent: YGNodeRef) -> io::Result<YGNodeRef> {
        let node = YGNodeNew();
        if node.is_null() { return Err(io::Error::other("Could not allocate Yoga layout")); }
        YGNodeInsertChild(parent, node, YGNodeGetChildCount(parent));
        Ok(node)
    }
}

impl Drop for Tree {
    fn drop(&mut self) { unsafe { YGNodeFreeRecursive(self.root); } }
}

unsafe fn rect(node: YGNodeRef) -> Rect {
    Rect {
        left: YGNodeLayoutGetLeft(node).round() as i32,
        top: YGNodeLayoutGetTop(node).round() as i32,
        width: YGNodeLayoutGetWidth(node).round() as i32,
        height: YGNodeLayoutGetHeight(node).round() as i32,
    }
}

pub(crate) fn dashboard(width: i32, height: i32, dpi: i32, typography: Typography, toggle_width: i32) -> io::Result<DashboardLayout> {
    let s = |value: i32| (value * dpi / 96).max(1);
    let width = width.max(1);
    let height = height.max(1);
    let tiny = height < s(150);
    let short = height < s(290);
    let margin = s(if tiny { 6 } else if short { 10 } else { 18 }).min((width - 1) / 2).min(height / 2);
    let header_height = typography.cell.max(typography.advance).min((height - margin - 1).max(0));
    let gap = s(if tiny { 5 } else if short { 8 } else { 12 }).min((height - margin - header_height - 1).max(0));
    let tree = Tree::new()?;
    unsafe {
        YGNodeStyleSetWidth(tree.root, width as f32);
        YGNodeStyleSetHeight(tree.root, height as f32);
        YGNodeStyleSetPadding(tree.root, YGEdgeTop, margin as f32);
        let header = tree.child(tree.root)?;
        YGNodeStyleSetFlexDirection(header, YGFlexDirectionRow);
        YGNodeStyleSetHeight(header, header_height as f32);
        YGNodeStyleSetMargin(header, YGEdgeBottom, gap as f32);
        YGNodeStyleSetMargin(header, YGEdgeHorizontal, margin as f32);
        let status = tree.child(header)?;
        YGNodeStyleSetFlexGrow(status, 1.0);
        YGNodeStyleSetFlexBasis(status, 0.0);
        YGNodeStyleSetMinWidth(status, 1.0);
        YGNodeStyleSetFlexShrink(status, 1.0);
        let toggle = tree.child(header)?;
        YGNodeStyleSetWidth(toggle, toggle_width.max(0) as f32);
        YGNodeStyleSetFlexShrink(toggle, 1.0);
        let body = tree.child(tree.root)?;
        YGNodeStyleSetFlexDirection(body, YGFlexDirectionRow);
        YGNodeStyleSetFlexGrow(body, 1.0);
        YGNodeStyleSetMinHeight(body, 0.0);
        YGNodeStyleSetMargin(body, YGEdgeHorizontal, margin as f32);
        let viewport = tree.child(body)?;
        YGNodeStyleSetFlexGrow(viewport, 1.0);
        YGNodeStyleSetMinWidth(viewport, 0.0);
        let scrollbar = tree.child(body)?;
        let scrollbar_width = s(8).min((margin - s(2)).max(1));
        YGNodeStyleSetPositionType(scrollbar, YGPositionTypeAbsolute);
        YGNodeStyleSetPosition(scrollbar, YGEdgeTop, 0.0);
        YGNodeStyleSetPosition(scrollbar, YGEdgeBottom, 0.0);
        YGNodeStyleSetPosition(scrollbar, YGEdgeRight, -((margin + scrollbar_width) / 2) as f32);
        YGNodeStyleSetWidth(scrollbar, scrollbar_width as f32);
        YGNodeCalculateLayout(tree.root, width as f32, height as f32, YGDirectionLTR);
        let header_rect = rect(header);
        let body_rect = rect(body);
        Ok(DashboardLayout {
            status: rect(status).offset(header_rect),
            toggle: rect(toggle).offset(header_rect),
            viewport: rect(viewport).offset(body_rect),
            scrollbar: rect(scrollbar).offset(body_rect),
        })
    }
}

pub(crate) fn content(width: i32, dpi: i32, typography: Typography, sizes: &[RowSize]) -> io::Result<ContentLayout> {
    let s = |value: i32| (value * dpi / 96).max(1);
    let width = width.max(1);
    let tree = Tree::new()?;
    unsafe {
        YGNodeStyleSetWidth(tree.root, width as f32);
        let mut nodes = Vec::with_capacity(sizes.len());
        for (index, size) in sizes.iter().enumerate() {
            let row = tree.child(tree.root)?;
            let height = size.height.max(1);
            YGNodeStyleSetHeight(row, height as f32);
            YGNodeStyleSetFlexShrink(row, 0.0);
            if index + 1 < sizes.len() { YGNodeStyleSetMargin(row, YGEdgeBottom, size.spacing.gap_after as f32); }
            let action = if let Some(action_width) = size.action {
                let action = tree.child(row)?;
                let action_width = action_width.max(1).min(width);
                let action_height = typography.cell.max(typography.advance).min(height);
                YGNodeStyleSetPositionType(action, YGPositionTypeAbsolute);
                YGNodeStyleSetPosition(action, YGEdgeRight, s(10).min(width - action_width) as f32);
                YGNodeStyleSetPosition(action, YGEdgeTop, size.spacing.top.min(height - action_height) as f32);
                YGNodeStyleSetWidth(action, action_width as f32);
                YGNodeStyleSetHeight(action, action_height as f32);
                Some(action)
            } else { None };
            nodes.push((row, action));
        }
        YGNodeCalculateLayout(tree.root, width as f32, f32::NAN, YGDirectionLTR);
        Ok(ContentLayout {
            rows: nodes.into_iter().map(|(row, action)| RowLayout { rect: rect(row), action: action.map(|node| rect(node)) }).collect(),
            height: rect(tree.root).height,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_fits_at_multiple_sizes_and_dpis() {
        for dpi in [96, 120, 144, 192] {
            for (width, height) in [(900, 700), (420, 250), (180, 110), (20, 20)] {
                let layout = dashboard(width, height, dpi, Typography::new(dpi, 15 * dpi / 96), 112 * dpi / 96).unwrap();
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
    fn rows_stack_and_actions_stay_inside_measured_rows() {
        for dpi in [96, 120, 144, 192] {
            for width in [1, 20, 120, 800] {
                let spacing = RowSpacing::new(Typography::new(dpi, 15 * dpi / 96), false, false);
                let sizes = [RowSize { height: 40, action: None, spacing }, RowSize { height: 64, action: Some(78), spacing }, RowSize { height: 220, action: Some(78), spacing }];
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
                    RowSize { height: heading.height(typography, heading_lines), action: None, spacing: heading },
                    RowSize { height: branch.height(typography, 3), action: Some(78), spacing: branch },
                    RowSize { height: separate.height(typography, 2), action: Some(78), spacing: separate },
                    RowSize { height: separate.height(typography, 1), action: None, spacing: separate },
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
                        RowSize { height: attached.height(typography, lines), action: Some(78), spacing: attached },
                        RowSize { height: last.height(typography, 2), action: Some(78), spacing: last },
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
                let hidden = dashboard(width, 540, dpi, typography, 0).unwrap();
                let visible = dashboard(width, 540, dpi, typography, 100).unwrap();
                assert_eq!(hidden.status.left, visible.status.left);
                assert!(hidden.status.width > visible.status.width);
                assert_eq!(hidden.toggle.width, 0);
                assert_eq!(hidden.viewport, visible.viewport);
                assert_eq!(hidden.status.top, visible.status.top);
                assert_eq!(hidden.status.height, visible.status.height);
                let spacing = RowSpacing::new(typography, true, true);
                let hidden = content(width, dpi, typography, &[RowSize { height: 60, action: None, spacing }]).unwrap();
                let visible = content(width, dpi, typography, &[RowSize { height: 60, action: Some(60), spacing }]).unwrap();
                assert_eq!(hidden.rows[0].rect, visible.rows[0].rect);
                assert!(hidden.rows[0].action.is_none());
                assert_eq!(visible.rows[0].action.unwrap().width, 60);
            }
        }
    }
}
