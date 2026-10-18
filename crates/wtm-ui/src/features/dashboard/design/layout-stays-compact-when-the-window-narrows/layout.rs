use std::io;
use std::sync::{Mutex, MutexGuard};
use yoga_sys::*;
use yoga_sys::YGDirection::YGDirectionLTR;
use yoga_sys::YGEdge::YGEdgeBottom;

static YOGA: Mutex<()> = Mutex::new(());

struct Tree {
    root: YGNodeRef,
    _guard: MutexGuard<'static, ()>,
}

impl Tree {
    fn new() -> io::Result<Self> {
        let guard = YOGA.lock().map_err(|_| io::Error::other("Yoga layout lock poisoned"))?;
        let root = unsafe { YGNodeNew() };
        if root.is_null() { Err(io::Error::other("Could not allocate Yoga layout")) } else { Ok(Self { root, _guard: guard }) }
    }

    unsafe fn child(&self) -> io::Result<YGNodeRef> {
        let node = YGNodeNew();
        if node.is_null() { return Err(io::Error::other("Could not allocate Yoga layout")); }
        YGNodeInsertChild(self.root, node, YGNodeGetChildCount(self.root));
        Ok(node)
    }
}

impl Drop for Tree {
    fn drop(&mut self) { unsafe { YGNodeFreeRecursive(self.root); } }
}

pub(crate) fn content_rows(heights: &[usize], attached: &[bool]) -> io::Result<(Vec<usize>, usize)> {
    let tree = Tree::new()?;
    unsafe {
        YGNodeStyleSetWidth(tree.root, 1.0);
        let mut nodes = Vec::with_capacity(heights.len());
        for (index, &height) in heights.iter().enumerate() {
            let row = tree.child()?;
            YGNodeStyleSetHeight(row, height as f32);
            YGNodeStyleSetFlexShrink(row, 0.0);
            if index + 1 < heights.len() && !attached.get(index).copied().unwrap_or(false) {
                YGNodeStyleSetMargin(row, YGEdgeBottom, 1.0);
            }
            nodes.push(row);
        }
        YGNodeCalculateLayout(tree.root, 1.0, f32::NAN, YGDirectionLTR);
        let offsets = nodes.into_iter().map(|row| YGNodeLayoutGetTop(row).round() as usize).collect();
        Ok((offsets, YGNodeLayoutGetHeight(tree.root).round() as usize))
    }
}

#[cfg(test)]
#[path = "layout.test.rs"]
mod tests;
