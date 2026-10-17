use std::io;
use crate::typography::{RowSpacing, Typography};
use std::sync::{Mutex, MutexGuard};
use yoga_sys::*;
use yoga_sys::YGDirection::YGDirectionLTR;
use yoga_sys::YGEdge::{YGEdgeBottom, YGEdgeLeft, YGEdgeRight, YGEdgeTop};
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
    pub action_width: Option<i32>,
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

pub(crate) fn dashboard(width: i32, height: i32, dpi: i32, typography: Typography, space_width: i32, toggle_width: i32) -> io::Result<DashboardLayout> {
    let s = |value: i32| (value * dpi / 96).max(1);
    let width = width.max(1);
    let height = height.max(1);
    let tiny = height < s(150);
    let short = height < s(290);
    let margin = s(if tiny { 6 } else if short { 10 } else { 18 }).min((width - 1) / 2).min(height / 2);
    let left = (margin - 2 * space_width.max(0)).max(0);
    let right = (margin - space_width.max(0)).max(0);
    let top = (margin - typography.cell).max(0);
    let header_height = typography.cell.max(typography.advance).min((height - top - 1).max(0));
    let gap = s(if tiny { 5 } else if short { 8 } else { 12 }).min((height - top - header_height - 1).max(0));
    let tree = Tree::new()?;
    unsafe {
        YGNodeStyleSetWidth(tree.root, width as f32);
        YGNodeStyleSetHeight(tree.root, height as f32);
        YGNodeStyleSetPadding(tree.root, YGEdgeTop, top as f32);
        let header = tree.child(tree.root)?;
        YGNodeStyleSetFlexDirection(header, YGFlexDirectionRow);
        YGNodeStyleSetHeight(header, header_height as f32);
        YGNodeStyleSetMargin(header, YGEdgeBottom, gap as f32);
        YGNodeStyleSetMargin(header, YGEdgeLeft, left as f32);
        YGNodeStyleSetMargin(header, YGEdgeRight, right as f32);
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
        YGNodeStyleSetMargin(body, YGEdgeLeft, left as f32);
        let viewport = tree.child(body)?;
        YGNodeStyleSetFlexGrow(viewport, 1.0);
        YGNodeStyleSetMinWidth(viewport, 0.0);
        let gutter = tree.child(body)?;
        let gutter_width = right.max(s(8)).min(width - left);
        YGNodeStyleSetWidth(gutter, gutter_width as f32);
        YGNodeStyleSetFlexShrink(gutter, 0.0);
        let scrollbar = tree.child(gutter)?;
        let scrollbar_width = s(8).min(gutter_width);
        YGNodeStyleSetPositionType(scrollbar, YGPositionTypeAbsolute);
        YGNodeStyleSetPosition(scrollbar, YGEdgeTop, 0.0);
        YGNodeStyleSetPosition(scrollbar, YGEdgeBottom, 0.0);
        YGNodeStyleSetPosition(scrollbar, YGEdgeLeft, ((gutter_width - scrollbar_width) / 2) as f32);
        YGNodeStyleSetWidth(scrollbar, scrollbar_width as f32);
        YGNodeCalculateLayout(tree.root, width as f32, height as f32, YGDirectionLTR);
        let header_rect = rect(header);
        let body_rect = rect(body);
        Ok(DashboardLayout {
            status: rect(status).offset(header_rect),
            toggle: rect(toggle).offset(header_rect),
            viewport: rect(viewport).offset(body_rect),
            scrollbar: rect(scrollbar).offset(rect(gutter)).offset(body_rect),
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
            let action = if let Some(action_width) = size.action_width {
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
#[path = "layout.test.rs"]
mod tests;
