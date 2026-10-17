use crate::rendering::{text_row, GREEN, MUTED, TEAL, WHITE};
use super::buffer_presentation;
use super::dashboard_rows::{Card};
use super::row_layout::{layout};
use super::runtime::{State, mouse_point, scaled};
use super::window_fading::{track_hover};
use super::window_movement::{edge_hit_transparent};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{BeginPaint, EndPaint, HDC, InvalidateRect, PAINTSTRUCT};
use windows_sys::Win32::UI::Controls::{BST_CHECKED, DRAWITEMSTRUCT, ODS_CHECKED, ODS_DISABLED, ODS_FOCUS, ODS_GRAYED, ODS_HOTLIGHT, ODS_SELECTED};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetFocus, IsWindowEnabled, SetFocus};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{BM_GETCHECK, BM_GETSTATE, BM_SETCHECK, BM_SETSTATE, BST_PUSHED, GetClientRect, GetForegroundWindow, GetWindowTextW, HTTRANSPARENT, IsIconic, SWP_NOACTIVATE, SWP_NOCOPYBITS, SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, SendMessageW, SetWindowPos, ShowWindow, WM_ENABLE, WM_ERASEBKGND, WM_KILLFOCUS, WM_NCDESTROY, WM_NCHITTEST, WM_PAINT, WM_PRINTCLIENT, WM_SETFOCUS};
pub(super) unsafe fn position_action(card: &Card) {
    let Some(action) = card.action else { return; };
    let Some(rect) = card.action_rect else { return; };
    SetWindowPos(action, null_mut(), rect.left, rect.top, rect.width, rect.height,
        SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS);
}

pub(super) unsafe fn button_caption(window: HWND) -> String {
    let mut value = [0u16; 128];
    let length = GetWindowTextW(window, value.as_mut_ptr(), value.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&value[..length.min(value.len())])
}


pub(super) unsafe fn paint_button(draw: &DRAWITEMSTRUCT, state: &State) {
    let Some(resources) = &state.resources else { return; };
    let rect = draw.rcItem;
    let disabled = draw.itemState & (ODS_DISABLED | ODS_GRAYED) != 0;
    let pressed = draw.itemState & ODS_SELECTED != 0;
    let focused = draw.itemState & ODS_FOCUS != 0;
    let hot = draw.itemState & ODS_HOTLIGHT != 0;
    let checked = draw.itemState & ODS_CHECKED != 0;
    let color = if disabled { MUTED } else if checked || pressed { GREEN }
        else if focused || hot { TEAL } else { WHITE };
    let columns = buffer_presentation::columns(rect.right - rect.left - scaled(16, state.dpi), resources);
    let buffer = text_row(&button_caption(draw.hwndItem), columns, color);
    let used = buffer.row(0).unwrap().iter().rposition(|cell| cell.text() != " ").map_or(0, |i| i + 1);
    let left = rect.left + (rect.right - rect.left - used as i32 * resources.space_width) / 2;
    let top = rect.top + (rect.bottom - rect.top - resources.typography.cell).max(0) / 2;
    buffer_presentation::paint_at(&buffer, draw.hDC, rect, resources, left, top, buffer_presentation::TextLayer::All);
}

pub(super) unsafe fn paint_checkbox(dc: HDC, rect: RECT, window: HWND, state: &State) {
    let Some(resources) = &state.resources else { return; };
    let checked = SendMessageW(window, BM_GETCHECK, 0, 0) == BST_CHECKED as LRESULT;
    let focused = GetFocus() == window;
    let disabled = IsWindowEnabled(window) == 0;
    let pressed = SendMessageW(window, BM_GETSTATE, 0, 0) & BST_PUSHED as LRESULT != 0;
    let buffer = text_row(if checked { "[x] Show all" } else { "[ ] Show all" },
        buffer_presentation::columns(rect.right - rect.left, resources),
        if disabled { MUTED } else if pressed { GREEN } else if focused { TEAL }
            else if checked { GREEN } else { WHITE });
    let top = rect.top + (rect.bottom - rect.top - resources.typography.cell).max(0) / 2;
    buffer_presentation::paint_at(&buffer, dc, rect, resources, rect.left, top, buffer_presentation::TextLayer::All);
}

pub(super) unsafe extern "system" fn show_all_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, data: usize) -> LRESULT {
    track_hover(window, message);
    if message == WM_ERASEBKGND { return 1; }
    if message == WM_NCDESTROY { RemoveWindowSubclass(window, Some(show_all_procedure), 1); }
    if message == WM_NCHITTEST && edge_hit_transparent(window, mouse_point(lparam)) {
        return HTTRANSPARENT as LRESULT;
    }
    if message == WM_PAINT || message == WM_PRINTCLIENT {
        let mut paint: PAINTSTRUCT = std::mem::zeroed();
        let dc = if message == WM_PAINT { BeginPaint(window, &mut paint) } else { wparam as HDC };
        let mut rect: RECT = std::mem::zeroed();
        if !dc.is_null() && GetClientRect(window, &mut rect) != 0 {
            let state = &*(data as *const State);
            paint_checkbox(dc, rect, window, state);
        }
        if message == WM_PAINT { EndPaint(window, &paint); }
        return 0;
    }
    let result = DefSubclassProc(window, message, wparam, lparam);
    if matches!(message, WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE | BM_SETCHECK | BM_SETSTATE) {
        InvalidateRect(window, null(), 0);
    }
    result
}

pub(super) unsafe fn update_visibility_controls(window: HWND, state: &mut State, visible: bool) {
    state.controls_visible = visible;
    if visible && !state.indicator_overlay.is_null() { ShowWindow(state.indicator_overlay, SW_HIDE); }
    let focus = GetFocus();
    if !visible && (focus == state.show_all_button || state.cards.iter().any(|card| card.action == Some(focus))) {
        SetFocus(if GetForegroundWindow() == window && IsIconic(window) == 0 { state.viewport } else { null_mut() });
    }
    ShowWindow(state.show_all_button, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
    for card in &state.cards {
        if let Some(action) = card.action { ShowWindow(action, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE }); }
    }
    layout(window, state, None, false);
}

