use super::{repository_ordering, worktree_ordering};
use super::dashboard_rows::{refresh};
use super::runtime::{State, callback, mouse_point, scaled};
use super::window_fading::{track_hover};
use super::window_movement::{edge_hit_transparent};
use crate::dashboard::ItemKey;
use crate::order::{Order, OrderMove};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::time::Instant;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{InvalidateRect, MapWindowPoints, RDW_ALLCHILDREN, RDW_INVALIDATE, RDW_NOERASE, RedrawWindow, ScreenToClient};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture, SetFocus};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{GWLP_USERDATA, GetClientRect, GetParent, GetSystemMetrics, GetWindowLongPtrW, HTCLIENT, HTTRANSPARENT, PostMessageW, SM_CXDRAG, SM_CYDRAG, SendMessageW, WM_CANCELMODE, WM_CLOSE, WM_COMMAND, WM_CTLCOLORBTN, WM_DRAWITEM, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_NCDESTROY, WM_NCHITTEST};
pub(super) fn reorderable(state: &State, key: &ItemKey) -> bool {
    let Some(update) = &state.latest else { return false; };
    match key {
        ItemKey::Repository(repository) => update.snapshot.repos.iter()
            .any(|repo| crate::order::identity(&repo.common_dir) == *repository),
        ItemKey::Worktree(repository, path) => update.snapshot.repos.iter()
            .find(|repo| crate::order::identity(&repo.common_dir) == *repository)
            .and_then(|repo| repo.worktrees.as_ref().ok())
            .is_some_and(|trees| trees.iter().any(|tree| crate::order::identity(&tree.path) == *path)),
        _ => false,
    }
}

pub(super) fn project_marker(state: &State, marker: i32) -> Option<RECT> {
    let thickness = scaled(2, state.dpi);
    let y = marker.saturating_sub(state.scrolling.position);
    let rect = RECT { left: 0, top: y.saturating_sub(thickness).max(0), right: state.scrolling.width, bottom: y.min(state.scrolling.page) };
    (state.scrolling.width > 0 && state.scrolling.page > 0 && rect.left < rect.right && rect.top < rect.bottom).then_some(rect)
}

pub(super) fn same_rect(left: Option<&RECT>, right: Option<&RECT>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left.left == right.left && left.top == right.top
            && left.right == right.right && left.bottom == right.bottom,
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

pub(super) unsafe fn invalidate_drag(state: &State, source_handle: Option<HWND>, old_marker: Option<RECT>, new_marker: Option<RECT>) {
    if let Some(source_handle) = source_handle {
        InvalidateRect(source_handle, null(), 0);
    }
    if !same_rect(old_marker.as_ref(), new_marker.as_ref()) {
        for marker in old_marker.as_ref().into_iter().chain(new_marker.as_ref()) {
            RedrawWindow(state.viewport, marker, null_mut(), RDW_INVALIDATE | RDW_NOERASE | RDW_ALLCHILDREN);
        }
    }
}

pub(super) unsafe fn cancel_drag(pointer: *mut State) {
    if let Some(drag) = (*pointer).ordering.drag.take() {
        let viewport = (*pointer).viewport;
        invalidate_drag(&*pointer, Some(drag.source_handle), drag.marker_rect, None);
        if GetCapture() == viewport { ReleaseCapture(); }
    }
}

pub(super) unsafe fn move_drag(state: &mut State, point: POINT) {
    let Some(drag) = &state.ordering.drag else { return; };
    let active = drag.active || (point.x - drag.origin.x).abs() >= GetSystemMetrics(SM_CXDRAG).max(1)
        || (point.y - drag.origin.y).abs() >= GetSystemMetrics(SM_CYDRAG).max(1);
    let target = if active { drop_target(state, &drag.source, point) } else { None };
    let old_marker = drag.marker_rect;
    let new_marker = target.as_ref().and_then(|target| project_marker(state, target.marker));
    let drag = state.ordering.drag.as_mut().unwrap();
    drag.position = point;
    drag.active = active;
    drag.target = target;
    drag.marker_rect = new_marker;
    if !same_rect(old_marker.as_ref(), new_marker.as_ref()) {
        invalidate_drag(state, None, old_marker, new_marker);
    }
}

pub(super) unsafe fn finish_drag(pointer: *mut State, point: POINT) {
    move_drag(&mut *pointer, point);
    let drag = (*pointer).ordering.drag.take();
    let movement = drag.as_ref().and_then(|drag| drag.target.as_ref().map(|target| target.movement.clone()));
    let source_handle = drag.as_ref().map(|drag| drag.source_handle);
    let marker_rect = drag.as_ref().and_then(|drag| drag.marker_rect);
    invalidate_drag(&*pointer, source_handle, marker_rect, None);
    let viewport = (*pointer).viewport;
    if GetCapture() == viewport { ReleaseCapture(); }
    if let (Some(movement), Some(update)) = (movement, (*pointer).latest.clone()) {
        let state = &mut *pointer;
        if matches!(state.ordering.value.moved(&update.snapshot, &movement), Ok(None)) { return; }
        super::preference_failures::save_order(state, &update.snapshot, &movement);
        refresh(GetParent(viewport), state);
    }
}

pub(super) unsafe extern "system" fn card_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    track_hover(window, message);
    if matches!(message, WM_COMMAND | WM_DRAWITEM | WM_CTLCOLORBTN) {
        let viewport = GetParent(window);
        if !viewport.is_null() && !(GetWindowLongPtrW(viewport, GWLP_USERDATA) as *mut State).is_null() {
            return callback(viewport, message, wparam, lparam, true);
        }
    }
    if message == WM_ERASEBKGND { return 1; }
    if message == WM_NCDESTROY {
        let viewport = GetParent(window);
        if GetCapture() == viewport { SendMessageW(viewport, WM_CANCELMODE, 0, 0); }
        RemoveWindowSubclass(window, Some(card_procedure), 1);
    }
    if message == WM_NCHITTEST {
        if edge_hit_transparent(window, mouse_point(lparam)) {
            return HTTRANSPARENT as LRESULT;
        }
        let mut point = mouse_point(lparam);
        ScreenToClient(window, &mut point);
        let mut rect: RECT = std::mem::zeroed();
        GetClientRect(window, &mut rect);
        if point.x >= 0 && point.x < rect.right
            && point.y >= 0 && point.y < rect.bottom { return HTCLIENT as LRESULT; }
    }
    if message == WM_LBUTTONDOWN {
        let viewport = GetParent(window);
        let pointer = GetWindowLongPtrW(viewport, GWLP_USERDATA) as *mut State;
        if !pointer.is_null() {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let mut point = mouse_point(lparam);
                let mut rect: RECT = std::mem::zeroed();
                GetClientRect(window, &mut rect);
                if point.x < 0 || point.x >= rect.right || point.y < 0 || point.y >= rect.bottom { return false; }
                let source = (*pointer).cards.iter().find(|card| card.window == window).and_then(|card| card.key.clone());
                if let Some(source) = source.filter(|key| reorderable(&*pointer, key)) {
                    cancel_drag(pointer);
                    MapWindowPoints(window, viewport, &mut point, 1);
                    SetFocus(viewport);
                    SetCapture(viewport);
                    if GetCapture() == viewport {
                        (*pointer).ordering.drag = Some(Drag { source, source_handle: window, origin: point, position: point,
                            active: false, target: None, marker_rect: None });
                        invalidate_drag(&*pointer, Some(window), None, None);
                    }
                    return true;
                }
                false
            }));
            match result {
                Ok(true) => return 0,
                Ok(false) => {},
                Err(_) => { cancel_drag(pointer); PostMessageW(GetParent(viewport), WM_CLOSE, 0, 0); return 0; }
            }
        }
    }
    DefSubclassProc(window, message, wparam, lparam)
}

pub(super) struct Drag {
    pub(super) source: ItemKey,
    pub(super) source_handle: HWND,
    pub(super) origin: POINT,
    pub(super) position: POINT,
    pub(super) active: bool,
    pub(super) target: Option<DropTarget>,
    pub(super) marker_rect: Option<RECT>,
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
