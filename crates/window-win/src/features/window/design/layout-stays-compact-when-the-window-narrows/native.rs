use crate::layout;
use super::dashboard_rows::{Card};
use super::font_resources::{Resources, wrapped_width};
use super::loading_rows::{pending_row_lines};
use super::row_scrolling::{set_scroll_info, update_scrollbar};
use super::scroll_anchor::restore_anchor;
use super::runtime::{State, scaled};
use super::visibility_controls::{button_caption, position_action};
use crate::dashboard::{DisplayItem, ItemKey};
use crate::layout::RowSize;
use crate::spacing::RowSpacing;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{GetDC, HDC, HFONT, InvalidateRect, RDW_ALLCHILDREN, RDW_FRAME, RDW_INVALIDATE, RedrawWindow, ReleaseDC};
use windows_sys::Win32::UI::WindowsAndMessaging::{BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos, GetClientRect, MoveWindow, PostMessageW, SWP_NOACTIVATE, SWP_NOCOPYBITS, SWP_NOZORDER, SetWindowPos, WM_CLOSE};
pub(super) fn content_inset(width: i32, dpi: i32, resources: &Resources) -> i32 {
    (scaled(14, dpi) - resources.space_width).max(0).min(width.saturating_sub(1) / 2)
}
pub(super) fn content_text_width(width: i32, dpi: i32, resources: &Resources) -> i32 {
    (width - content_inset(width, dpi, resources) * 2).max(1)
}

pub(super) unsafe fn wrapped_lines(dc: HDC, text: &str, width: i32, font: HFONT) -> Vec<String> {
    crate::wrapping::wrap(text, width, |value| wrapped_width(dc, value, font))
}

pub(super) unsafe fn item_height(card: &Card, dc: HDC, width: i32, dpi: i32, resources: &Resources) -> i32 {
    let lines = match &card.display {
        DisplayItem::Repository { bare, .. } => 1 + usize::from(*bare),
        DisplayItem::PendingWorktree { .. } => pending_row_lines(),
        DisplayItem::Worktree { result, .. } => {
            let text_width = content_text_width(width, dpi, resources);
            match result {
                Ok(_) => 2,
                Err(error) => 2 + wrapped_lines(dc, &format!("ERROR ({:?}): {}", error.kind, error.message), text_width, resources.font).len(),
            }
        }
        DisplayItem::Notice { message, .. } => 1 + if message.is_empty() { 0 } else {
            let text_width = content_text_width(width, dpi, resources);
            wrapped_lines(dc, message, text_width, resources.font).len()
        },
        _ => 1,
    };
    card.spacing.height(resources.typography, lines)
}

pub(super) fn attached_rows(previous: &DisplayItem, next: &DisplayItem) -> bool {
    matches!(previous, DisplayItem::Repository { .. } | DisplayItem::Worktree { .. } | DisplayItem::PendingWorktree { .. })
        && matches!(next, DisplayItem::Worktree { .. } | DisplayItem::PendingWorktree { .. })
}

pub(super) unsafe fn position_cards(state: &State) {
    let mut placements = Vec::with_capacity(state.cards.len());
    for card in &state.cards {
        let top = card.top - state.scrolling.position;
        placements.push((card.window, RECT { left: 0, top, right: state.scrolling.width, bottom: top + card.height }));
    }
    let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS;
    let mut batch = BeginDeferWindowPos(placements.len() as i32);
    for (window, rect) in &placements {
        if batch.is_null() { break; }
        batch = DeferWindowPos(batch, *window, null_mut(), rect.left, rect.top,
            rect.right - rect.left, rect.bottom - rect.top, flags);
    }
    if batch.is_null() || EndDeferWindowPos(batch) == 0 {
        for (window, rect) in placements {
            SetWindowPos(window, null_mut(), rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, flags);
        }
    }
    for card in &state.cards { position_action(card); }
    InvalidateRect(state.viewport, null(), 0);
}

pub(super) unsafe fn layout(window: HWND, state: &mut State, restore: Option<(ItemKey, i32)>, scrollbar_activity: bool) {
    state.indicators_dirty = true;
    if state.resources.is_none() { return; }
    if state.fixed.len() != 1 || state.viewport.is_null() { return; }
    let was_overflowing = state.scrolling.content > state.scrolling.page;
    let mut client: RECT = std::mem::zeroed();
    if GetClientRect(window, &mut client) == 0 || client.right <= 0 || client.bottom <= 0 { return; }
    let resources = state.resources.as_ref().unwrap();
    let dc = GetDC(window);
    let toggle_width = if state.controls_visible { wrapped_width(dc, "[ ] Show all", resources.font) } else { 0 };
    let space_width = wrapped_width(dc, " ", resources.font);
    ReleaseDC(window, dc);
    let dashboard = match layout::dashboard(client.right, client.bottom, state.dpi, resources.typography, space_width, toggle_width) {
        Ok(layout) => layout,
        Err(error) => { state.failure = Some(error); PostMessageW(window, WM_CLOSE, 0, 0); return; }
    };
    for (control, rect) in [(state.fixed[0].window, dashboard.status), (state.show_all_button, dashboard.toggle),
        (state.viewport, dashboard.viewport), (state.scrolling.control, dashboard.scrollbar)] {
        if !control.is_null() { MoveWindow(control, rect.left, rect.top, rect.width.max(1), rect.height.max(1), 0); }
    }
    let mut viewport: RECT = std::mem::zeroed();
    GetClientRect(state.viewport, &mut viewport);
    state.scrolling.width = viewport.right.max(1);
    state.scrolling.page = viewport.bottom.max(1);
    let resources = state.resources.as_ref().unwrap();
    let attachments: Vec<_> = state.cards.windows(2).map(|cards| attached_rows(&cards[0].display, &cards[1].display)).collect();
    for (index, card) in state.cards.iter_mut().enumerate() {
        card.spacing = RowSpacing::new(resources.typography,
            index > 0 && attachments[index - 1], attachments.get(index).copied().unwrap_or(false));
    }
    let dc = GetDC(window);
    let sizes: Vec<_> = state.cards.iter().map(|card| RowSize {
        height: item_height(card, dc, state.scrolling.width, state.dpi, resources),
        action_width: card.action.filter(|_| state.controls_visible).map(|action| wrapped_width(dc, &button_caption(action), resources.font) + scaled(16, state.dpi)),
        spacing: card.spacing,
    }).collect();
    ReleaseDC(window, dc);
    let content = match layout::content(state.scrolling.width, state.dpi, resources.typography, &sizes) {
        Ok(layout) => layout,
        Err(error) => { state.failure = Some(error); PostMessageW(window, WM_CLOSE, 0, 0); return; }
    };
    for (card, row) in state.cards.iter_mut().zip(content.rows) {
        card.top = row.rect.top;
        card.height = row.rect.height;
        card.action_rect = row.action;
    }
    state.scrolling.content = content.height;
    restore_anchor(state, restore.as_ref());
    state.scrolling.position = state.scrolling.position.clamp(0, (state.scrolling.content - state.scrolling.page).max(0));
    set_scroll_info(state);
    update_scrollbar(state, scrollbar_activity || !was_overflowing && state.scrolling.content > state.scrolling.page);
    position_cards(state);
    RedrawWindow(window, null(), null_mut(), RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_FRAME);
}

