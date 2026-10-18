use super::row_layout::layout;
use super::runtime::State;
use wtm_dashboard::dashboard::{DisplayItem, ItemKey};
use windows_sys::Win32::Foundation::{HWND, POINT, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Control { ShowAll, Worktree(ItemKey) }

pub(super) fn action_caption(card: &super::dashboard_rows::Card) -> &'static str {
    if card.action_hidden == Some(true) { "Unhide" } else { "Hide" }
}

pub(super) fn hit_test(state: &State, point: POINT) -> Option<Control> {
    if !state.controls_visible || point.x < 0 || point.y < 0 { return None; }
    let toggle = state.columns.min(12) as i32;
    if point.y == 0 && point.x >= state.columns as i32 - toggle && point.x < state.columns as i32 {
        return Some(Control::ShowAll);
    }
    if point.y < 2 || point.y >= state.rows as i32 { return None; }
    let row = point.y - 2 + state.scrolling.position;
    state.cards.iter().find_map(|card| {
        if matches!(card.display, DisplayItem::PendingWorktree { .. }) { return None; }
        let rect = card.action_rect?;
        (point.x >= rect.left && point.x < rect.left + rect.width && row >= card.top + rect.top
            && row < card.top + rect.top + rect.height).then(|| Control::Worktree(card.key.clone().unwrap()))
    })
}

pub(super) unsafe fn activate(state: &mut State, control: Control) {
    match control {
        Control::ShowAll => {
            state.show_all = !state.show_all;
            super::dashboard_rows::refresh(state.window, state);
        }
        Control::Worktree(ItemKey::Worktree(repository, worktree)) => {
            super::worktree_visibility::toggle_visibility(state.window, state, repository, worktree);
        }
        Control::Worktree(_) => return,
    }
    if let Some(Control::Worktree(key)) = &state.focused {
        if !state.cards.iter().any(|card| card.key.as_ref() == Some(key)) { state.focused = Some(Control::ShowAll); }
    }
    state.frame_dirty = true;
    layout(state.window, state, None, false);
}

pub(super) unsafe fn keyboard(state: &mut State, key: WPARAM, backwards: bool) -> bool {
    if key == 0x09 {
        if !state.controls_visible { update_visibility_controls(state.window, state, true); }
        let mut controls = vec![Control::ShowAll];
        controls.extend(state.cards.iter().filter(|card| card.action_hidden.is_some()
            && !matches!(card.display, DisplayItem::PendingWorktree { .. }))
            .filter_map(|card| card.key.clone().map(Control::Worktree)));
        let index = state.focused.as_ref().and_then(|focused| controls.iter().position(|control| control == focused));
        let next = match index {
            Some(index) if backwards => (index + controls.len() - 1) % controls.len(),
            Some(index) => (index + 1) % controls.len(),
            None if backwards => controls.len() - 1,
            None => 0,
        };
        state.focused = Some(controls[next].clone());
        if let Some(Control::Worktree(key)) = &state.focused {
            if let Some(card) = state.cards.iter().find(|card| card.key.as_ref() == Some(key)) {
                if card.top < state.scrolling.position { super::row_scrolling::scroll_to(state, card.top); }
                else if card.top >= state.scrolling.position + state.scrolling.page {
                    super::row_scrolling::scroll_to(state, card.top + 1 - state.scrolling.page);
                }
            }
        }
        state.frame_dirty = true;
        super::row_layout::invalidate_frame(state);
        return true;
    }
    if matches!(key, 0x0d | 0x20) {
        if let Some(control) = state.focused.clone() { activate(state, control); return true; }
    }
    false
}

pub(super) unsafe fn update_visibility_controls(window: HWND, state: &mut State, visible: bool) {
    if state.controls_visible == visible { return; }
    state.controls_visible = visible;
    if !visible {
        state.focused = None;
        if state.pressed.take().is_some() && GetCapture() == state.window { ReleaseCapture(); }
    }
    layout(window, state, None, false);
}

