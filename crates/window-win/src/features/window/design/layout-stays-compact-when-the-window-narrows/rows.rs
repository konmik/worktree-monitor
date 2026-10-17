use super::row_scrolling::update_scrollbar;
use super::scroll_anchor::restore_anchor;
use super::runtime::State;
use super::visibility_controls::action_caption;
use crate::dashboard::{DisplayItem, ItemKey};
use std::ptr::null;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

pub(super) fn attached_rows(previous: &DisplayItem, next: &DisplayItem) -> bool {
    matches!(previous, DisplayItem::Repository { .. } | DisplayItem::Worktree { .. } | DisplayItem::PendingWorktree { .. })
        && matches!(next, DisplayItem::Worktree { .. } | DisplayItem::PendingWorktree { .. })
}

pub(super) unsafe fn invalidate_frame(state: &mut State) {
    state.frame_dirty = true;
    state.indicators_dirty = true;
    InvalidateRect(state.window, null(), 0);
}

pub(super) unsafe fn layout(window: HWND, state: &mut State, restore: Option<(ItemKey, i32)>, scrollbar_activity: bool) {
    let Some(resources) = &state.resources else { return; };
    let was_overflowing = state.scrolling.content > state.scrolling.page;
    let mut client: RECT = std::mem::zeroed();
    if GetClientRect(window, &mut client) == 0 { return; }
    state.columns = (client.right.max(0) / resources.space_width.max(1)) as usize;
    state.rows = (client.bottom.max(0) / resources.typography.advance.max(1)) as usize;
    state.scrolling.width = state.columns.saturating_sub(1) as i32;
    state.scrolling.page = state.rows.saturating_sub(2) as i32;
    let frame = super::buffer_presentation::compose_frame(state);
    for (index, card) in state.cards.iter_mut().enumerate() {
        card.top = frame.row_offsets[index] as i32;
        card.height = frame.row_heights[index] as i32;
        card.action_rect = card.action_hidden.filter(|_| state.controls_visible).map(|_| {
            let width = (crate::render_buffer::display_width(action_caption(card)) + 2).min(state.scrolling.width.max(0) as usize) as i32;
            crate::layout::Rect { left: state.scrolling.width - width, top: 0, width, height: 1 }
        });
    }
    state.scrolling.content = frame.content_rows as i32;
    restore_anchor(state, restore.as_ref());
    state.scrolling.position = state.scrolling.position.clamp(0, (state.scrolling.content - state.scrolling.page).max(0));
    update_scrollbar(state, scrollbar_activity || !was_overflowing && state.scrolling.content > state.scrolling.page);
    invalidate_frame(state);
}
