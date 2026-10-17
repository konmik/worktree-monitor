use crate::render_buffer::RenderBuffer;
use crate::rendering::{render_row, RowOptions};
use super::buffer_presentation;
use super::font_resources::{Resources};
use super::idle_indicator::{set_status};
use super::refresh_markers::refresh_indicators;
use super::row_layout::{content_inset, content_text_width, layout};
use super::row_ordering::{card_procedure};
use super::runtime::{State, scaled, wide};
use super::scroll_anchor::{anchor};
use super::window_movement::{edge_procedure};
use super::worktree_visibility::{show_all_checked, update_action_caption};
use crate::dashboard::{DashboardView, DisplayItem, ItemKey};
use crate::layout::Rect;
use crate::spacing::RowSpacing;
use std::io;
use std::collections::HashMap;
use std::ptr::null;
use windows_sys::Win32::Foundation::{HWND, WPARAM};
use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::UI::Shell::SetWindowSubclass;
use windows_sys::Win32::UI::WindowsAndMessaging::{BS_OWNERDRAW, CreateWindowExW, DestroyWindow, PostMessageW, SW_HIDE, SW_SHOWNOACTIVATE, SendMessageW, SetWindowTextW, ShowWindow, WM_CLOSE, WM_SETFONT, WS_CHILD, WS_CLIPCHILDREN, WS_EX_CONTROLPARENT, WS_TABSTOP, WS_VISIBLE};
pub(super) unsafe fn set_item(card: &mut Card, display: DisplayItem) -> bool {
    if card.display != display {
        if matches!(card.display, DisplayItem::PendingWorktree { .. }) != matches!(display, DisplayItem::PendingWorktree { .. }) {
            if let Some(action) = card.action { EnableWindow(action, i32::from(!matches!(display, DisplayItem::PendingWorktree { .. }))); }
        }
        SetWindowTextW(card.window, wide(&display.caption_with_refresh(card.refreshing)).as_ptr());
        card.display = display;
        InvalidateRect(card.window, null(), 0);
        true
    } else { false }
}

pub(super) unsafe fn create_card(parent: HWND, id: usize, action_id: Option<usize>, key: Option<ItemKey>, display: DisplayItem, resources: &Resources) -> Option<Card> {
    let window = CreateWindowExW(WS_EX_CONTROLPARENT, wide("STATIC").as_ptr(), wide(&display.caption()).as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | STATIC_OWNERDRAW | STATIC_NOTIFY, 0, 0, 1, 1, parent, id as _, GetModuleHandleW(null()), null());
    if window.is_null() { return None; }
    let subclassed = if key.is_some() {
        SetWindowSubclass(window, Some(card_procedure), 1, 0)
    } else {
        SetWindowSubclass(window, Some(edge_procedure), 1, 0)
    };
    if subclassed == 0 {
        DestroyWindow(window);
        return None;
    }
    SendMessageW(window, WM_SETFONT, resources.font as WPARAM, 0);
    let action = if let Some(action_id) = action_id {
        let action = CreateWindowExW(0, wide("BUTTON").as_ptr(), wide("Hide").as_ptr(),
            WS_CHILD | WS_TABSTOP | BS_OWNERDRAW as u32, 0, 0, 1, 1, window,
            action_id as _, GetModuleHandleW(null()), null());
        if action.is_null() {
            DestroyWindow(window);
            return None;
        }
        if SetWindowSubclass(action, Some(edge_procedure), 1, 0) == 0 {
            DestroyWindow(action);
            DestroyWindow(window);
            return None;
        }
        SendMessageW(action, WM_SETFONT, resources.font as WPARAM, 0);
        if matches!(display, DisplayItem::PendingWorktree { .. }) { EnableWindow(action, 0); }
        Some(action)
    } else { None };
    Some(Card { window, action, action_hidden: None, key, display, refreshing: false, top: 0, height: 0, action_rect: None,
        spacing: RowSpacing::new(resources.typography, false, false) })
}

pub(super) unsafe fn destroy_card(card: Card) {
    DestroyWindow(card.window);
}

pub(super) unsafe fn reconcile(state: &mut State) -> bool {
    state.indicators_dirty = true;
    let Some(resources) = &state.resources else { return false; };
    let mut previous: HashMap<_, _> = std::mem::take(&mut state.cards).into_iter()
        .map(|card| (card.key.clone().unwrap(), card)).collect();
    let mut succeeded = true;
    for item in &state.view.items {
        if let Some(mut card) = previous.remove(&item.key) {
            set_item(&mut card, item.display.clone());
            update_action_caption(&mut card, &state.worktree_visibility.value);
            state.cards.push(card);
        } else {
            let action_id = matches!(item.key, ItemKey::Worktree(_, _)).then(|| {
                let id = state.next_action_id;
                state.next_action_id += 1;
                id
            });
            if let Some(mut card) = create_card(state.viewport, state.next_card_id, action_id, Some(item.key.clone()), item.display.clone(), resources) {
                state.next_card_id += 1;
                update_action_caption(&mut card, &state.worktree_visibility.value);
                if let Some(action) = card.action { ShowWindow(action, if state.controls_visible { SW_SHOWNOACTIVATE } else { SW_HIDE }); }
                state.cards.push(card);
            } else { succeeded = false; break; }
        }
    }
    for card in previous.into_values() { destroy_card(card); }
    succeeded
}

pub(super) unsafe fn refresh(window: HWND, state: &mut State) {
    let mut view = if let Some(update) = &state.latest {
        let mut targets = update.scan_targets.clone();
        state.ordering.value.apply_targets(&mut targets);
        let mut view = DashboardView::from_scan(&update.snapshot, &targets, &state.worktree_visibility.value, show_all_checked(state));
        if let Some(error) = &update.launch_error { view.launch_error(error); }
        view
    } else { DashboardView::scanning() };
    if let Some(warning) = &state.ordering.warning { view.presentation_warning(warning); }
    if let Some(warning) = &state.worktree_visibility.warning { view.visibility_warning(warning); }
    if view != state.view {
        let warning_changed = view.items.first().is_some_and(|item| matches!(item.key, ItemKey::Presentation | ItemKey::Visibility))
            && state.view.items.first() != view.items.first();
        let saved = if warning_changed { state.scrolling.position = 0; None } else { anchor(state) };
        state.view = view;
        if !reconcile(state) {
            state.failure = Some(io::Error::last_os_error());
            PostMessageW(window, WM_CLOSE, 0, 0);
            return;
        }
        layout(window, state, saved, false);
    }
    let mut action_caption_changed = false;
    for card in &mut state.cards { action_caption_changed |= update_action_caption(card, &state.worktree_visibility.value); }
    if action_caption_changed && state.controls_visible { layout(window, state, None, false); }
    refresh_indicators(state);
    set_status(state);
}

pub(super) fn render_card(card: &Card, state: &State, width: i32) -> Option<(RenderBuffer, i32, i32)> {
    let resources = state.resources.as_ref()?;
    let inset = content_inset(width, state.dpi, resources);
    let right_padding = (scaled(12, state.dpi) - resources.space_width).max(0);
    let heading_right = if matches!(card.display, DisplayItem::Repository { .. }) { width - right_padding }
        else { card.action_rect.map_or(width - right_padding, |rect| rect.left - scaled(8, state.dpi)) };
    let text_width = match card.display {
        DisplayItem::Status { .. } => width,
        DisplayItem::Worktree { .. } => content_text_width(width, state.dpi, resources),
        _ => width - inset - right_padding,
    };
    let dragging = state.ordering.drag.as_ref().is_some_and(|drag| drag.source_handle == card.window);
    let buffer = render_row(&card.display, buffer_presentation::columns(text_width, resources), RowOptions {
        refreshing: card.refreshing, dragging,
        heading_columns: Some(buffer_presentation::columns(heading_right - inset, resources)),
    });
    let left = if matches!(card.display, DisplayItem::Status { .. }) { 0 } else { inset };
    Some((buffer, left, card.spacing.top(resources.typography, 0)))
}

pub(super) struct Card {
    pub(super) window: HWND,
    pub(super) action: Option<HWND>,
    pub(super) action_hidden: Option<bool>,
    pub(super) key: Option<ItemKey>,
    pub(super) display: DisplayItem,
    pub(super) refreshing: bool,
    pub(super) top: i32,
    pub(super) height: i32,
    pub(super) action_rect: Option<Rect>,
    pub(super) spacing: RowSpacing,
}
pub(super) const STATIC_OWNERDRAW: u32 = 13;
pub(super) const STATIC_NOTIFY: u32 = 256;
