use super::{repository_ordering, worktree_ordering};
use super::dashboard_rows::refresh;
use super::runtime::State;
use wtm_ui::dashboard::ItemKey;
use wtm_ui::order::{Order, OrderMove};
use std::path::PathBuf;
use std::ptr::null;
use std::time::Instant;
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture, SetFocus};

pub(super) fn reorderable(state: &State, key: &ItemKey) -> bool {
    let Some(update) = &state.latest else { return false; };
    match key {
        ItemKey::Repository(repository) => update.snapshot.repos.iter()
            .any(|repo| wtm_ui::order::identity(&repo.common_dir) == *repository),
        ItemKey::Worktree(repository, path) => update.snapshot.repos.iter()
            .find(|repo| wtm_ui::order::identity(&repo.common_dir) == *repository)
            .and_then(|repo| repo.worktrees.as_ref().ok())
            .is_some_and(|trees| trees.iter().any(|tree| wtm_ui::order::identity(&tree.path) == *path)),
        _ => false,
    }
}

pub(super) fn drop_target(state: &State, source: &ItemKey, point: POINT) -> Option<DropTarget> {
    if point.x < 0 || point.x >= state.scrolling.width || point.y < 0 || point.y >= state.scrolling.page { return None; }
    match source {
        ItemKey::Repository(repository) => repository_ordering::drop_target(state, repository, point),
        ItemKey::Worktree(repository, worktree) => worktree_ordering::drop_target(state, repository, worktree, point),
        _ => None,
    }
}

pub(super) unsafe fn invalidate_drag(state: &mut State) {
    state.frame_dirty = true;
    state.indicators_dirty = true;
    InvalidateRect(state.window, null(), 0);
}

pub(super) unsafe fn begin_drag(state: &mut State, point: POINT) -> bool {
    if point.x < 0 || point.x >= state.scrolling.width || point.y < 0 || point.y >= state.scrolling.page { return false; }
    let y = point.y.saturating_add(state.scrolling.position);
    let source = state.cards.iter().find(|card| y >= card.top && y < card.top.saturating_add(card.height))
        .and_then(|card| card.key.clone()).filter(|key| reorderable(state, key));
    let Some(source) = source else { return false; };
    cancel_drag(state);
    SetFocus(state.window);
    SetCapture(state.window);
    if GetCapture() == state.window {
        state.ordering.drag = Some(Drag { source, origin: point, position: point, active: false, target: None });
        invalidate_drag(state);
    }
    true
}

pub(super) unsafe fn cancel_drag(pointer: *mut State) {
    let state = &mut *pointer;
    if state.ordering.drag.take().is_some() {
        invalidate_drag(state);
        if GetCapture() == state.window { ReleaseCapture(); }
    }
}

pub(super) unsafe fn move_drag(state: &mut State, point: POINT) {
    let Some(drag) = &state.ordering.drag else { return; };
    let active = drag.active || point.x != drag.origin.x || point.y != drag.origin.y;
    let target = if active { drop_target(state, &drag.source, point) } else { None };
    let changed = active != drag.active || target.as_ref().map(|target| target.marker) != drag.target.as_ref().map(|target| target.marker);
    let drag = state.ordering.drag.as_mut().unwrap();
    drag.position = point;
    drag.active = active;
    drag.target = target;
    if changed { invalidate_drag(state); }
}

pub(super) unsafe fn finish_drag(pointer: *mut State, point: POINT) {
    move_drag(&mut *pointer, point);
    let state = &mut *pointer;
    let drag = state.ordering.drag.take();
    let movement = drag.and_then(|drag| drag.target.map(|target| target.movement));
    invalidate_drag(state);
    if GetCapture() == state.window { ReleaseCapture(); }
    if let (Some(movement), Some(update)) = (movement, state.latest.clone()) {
        if matches!(state.ordering.value.moved(&update.snapshot, &movement), Ok(None)) { return; }
        super::preference_failures::save_order(state, &update.snapshot, &movement);
        refresh(state.window, state);
    }
}

pub(super) struct Drag {
    pub(super) source: ItemKey,
    pub(super) origin: POINT,
    pub(super) position: POINT,
    pub(super) active: bool,
    pub(super) target: Option<DropTarget>,
}

pub(super) struct DropTarget {
    pub(super) movement: OrderMove,
    pub(super) marker: i32,
}

pub(super) struct Ordering {
    pub(super) value: Order,
    pub(super) path: PathBuf,
    pub(super) warning: Option<String>,
    pub(super) write_failed: bool,
    pub(super) next_check: Instant,
    pub(super) drag: Option<Drag>,
}

impl Ordering {
    pub(super) fn load(path: PathBuf) -> Self {
        let (value, warning) = match Order::read(&path) {
            Ok(value) => (value, None),
            Err(error) => (Order::default(), Some(format!("Cannot load presentation order: {error}"))),
        };
        Self { value, path, warning, write_failed: false, next_check: Instant::now(), drag: None }
    }
}
