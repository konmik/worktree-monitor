use super::row_layout::{position_cards};
use super::row_ordering::{project_marker};
use super::runtime::{State, mouse_point, scaled};
use super::window_appearance::{MUTED, intersection};
use super::window_fading::{track_hover};
use super::window_movement::{edge_hit_transparent};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{BeginPaint, FillRect, HBRUSH, CreatePen, DeleteObject, EndPaint, HDC, InvalidateRect, LineTo, MoveToEx, PAINTSTRUCT, PS_SOLID, ScreenToClient, SelectObject, UpdateWindow};
use windows_sys::Win32::UI::Controls::{SetScrollInfo, WM_MOUSELEAVE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetCapture;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, GetScrollBarInfo, GetScrollInfo, GetWindowRect, HTCLIENT, HTTRANSPARENT, OBJID_CLIENT, SB_BOTTOM, SB_CTL, SB_LINEDOWN, SB_LINEUP, SB_PAGEDOWN, SB_PAGEUP, SB_THUMBPOSITION, SB_THUMBTRACK, SB_TOP, SCROLLBARINFO, SCROLLINFO, SIF_PAGE, SIF_POS, SIF_RANGE, SIF_TRACKPOS, SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow, WM_CANCELMODE, WM_CAPTURECHANGED, WM_ERASEBKGND, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCDESTROY, WM_NCHITTEST, WM_PAINT, WM_PRINTCLIENT, WM_SETFOCUS};
unsafe fn fill(dc: HDC, rect: RECT, brush: HBRUSH) { FillRect(dc, &rect, brush); }

pub(super) unsafe fn wake_scrollbar(state: &mut State) {
    if state.scrolling.control.is_null() { return; }
    if state.scrolling.content > state.scrolling.page {
        ShowWindow(state.scrolling.control, SW_SHOWNOACTIVATE);
        state.scrolling.idle = Some(Instant::now() + Duration::from_secs(3));
    } else {
        ShowWindow(state.scrolling.control, SW_HIDE);
        state.scrolling.idle = None;
    }
}

pub(super) unsafe fn update_scrollbar(state: &mut State, activity: bool) {
    if state.scrolling.control.is_null() { return; }
    if state.scrolling.content <= state.scrolling.page {
        ShowWindow(state.scrolling.control, SW_HIDE);
        state.scrolling.idle = None;
    } else if activity {
        wake_scrollbar(state);
    }
}

pub(super) unsafe fn set_scroll_info(state: &State) {
    let info = SCROLLINFO { cbSize: std::mem::size_of::<SCROLLINFO>() as u32, fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
        nMin: 0, nMax: (state.scrolling.content - 1).max(0), nPage: state.scrolling.page.max(0) as u32, nPos: state.scrolling.position, ..std::mem::zeroed() };
    if !state.scrolling.control.is_null() {
        SetScrollInfo(state.scrolling.control, SB_CTL, &info, 1);
        InvalidateRect(state.scrolling.control, null(), 0);
    }
}

pub(super) unsafe fn scroll_to(state: &mut State, position: i32) {
    let position = position.clamp(0, (state.scrolling.content - state.scrolling.page).max(0));
    if position != state.scrolling.position {
        state.indicators_dirty = true;
        state.scrolling.position = position;
        set_scroll_info(state);
        position_cards(state);
        let marker = state.ordering.drag.as_ref().and_then(|drag| drag.target.as_ref().map(|target| target.marker));
        let marker_rect = marker.and_then(|marker| project_marker(state, marker));
        if let Some(drag) = state.ordering.drag.as_mut() { drag.marker_rect = marker_rect; }
    }
    wake_scrollbar(state);
}

pub(super) unsafe fn handle_scroll(state: &mut State, wparam: WPARAM) {
    wake_scrollbar(state);
    let code = (wparam & 0xffff) as i32;
    let line = scaled(32, state.dpi);
    let page = (state.scrolling.page - line).max(line);
    let target = match code {
        SB_LINEUP => state.scrolling.position.saturating_sub(line),
        SB_LINEDOWN => state.scrolling.position.saturating_add(line),
        SB_PAGEUP => state.scrolling.position.saturating_sub(page),
        SB_PAGEDOWN => state.scrolling.position.saturating_add(page),
        SB_TOP => 0,
        SB_BOTTOM => state.scrolling.content,
        SB_THUMBTRACK | SB_THUMBPOSITION => {
            let mut info = SCROLLINFO { cbSize: std::mem::size_of::<SCROLLINFO>() as u32, fMask: SIF_TRACKPOS, ..std::mem::zeroed() };
            if state.scrolling.control.is_null() || GetScrollInfo(state.scrolling.control, SB_CTL, &mut info) == 0 { return; }
            info.nTrackPos
        }
        _ => return,
    };
    scroll_to(state, target);
}

pub(super) unsafe fn wheel(state: &mut State, wparam: WPARAM) {
    state.scrolling.wheel_remainder += ((wparam >> 16) as u16 as i16) as i32;
    let steps = state.scrolling.wheel_remainder / 120;
    state.scrolling.wheel_remainder %= 120;
    scroll_to(state, state.scrolling.position.saturating_sub(steps * scaled(72, state.dpi)));
}

pub(super) unsafe fn keyboard(state: &mut State, key: WPARAM) -> bool {
    let code = match key as u32 {
        0x26 => SB_LINEUP, 0x28 => SB_LINEDOWN, 0x21 => SB_PAGEUP, 0x22 => SB_PAGEDOWN,
        0x24 => SB_TOP, 0x23 => SB_BOTTOM, _ => return false,
    };
    handle_scroll(state, code as WPARAM);
    true
}

pub(super) unsafe fn draw_scrollbar_chevron(dc: HDC, rect: RECT, up: bool) {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 3 || height < 3 { return; }
    let half_width = ((width - 2).min(6) / 2).max(1);
    let half_height = ((height - 2).min(6) / 2).max(1);
    let center_x = (rect.left + rect.right) / 2;
    let center_y = (rect.top + rect.bottom) / 2;
    if up {
        MoveToEx(dc, center_x - half_width, center_y + half_height, null_mut());
        LineTo(dc, center_x, center_y - half_height);
        LineTo(dc, center_x + half_width, center_y + half_height);
    } else {
        MoveToEx(dc, center_x - half_width, center_y - half_height, null_mut());
        LineTo(dc, center_x, center_y + half_height);
        LineTo(dc, center_x + half_width, center_y - half_height);
    }
}

pub(super) unsafe fn paint_scrollbar(window: HWND, state: &State, message: u32, wparam: WPARAM) {
    let mut paint: PAINTSTRUCT = std::mem::zeroed();
    let dc = if message == WM_PAINT { BeginPaint(window, &mut paint) } else { wparam as HDC };
    if !dc.is_null() {
        if let Some(resources) = &state.resources {
            let mut client: RECT = std::mem::zeroed();
            if GetClientRect(window, &mut client) != 0 {
                fill(dc, client, resources.palette.background);
                let mut info = SCROLLBARINFO { cbSize: std::mem::size_of::<SCROLLBARINFO>() as u32, ..std::mem::zeroed() };
                if GetScrollBarInfo(window, OBJID_CLIENT, &mut info) != 0 {
                    let mut origin = POINT { x: info.rcScrollBar.left, y: info.rcScrollBar.top };
                    let mut end = POINT { x: info.rcScrollBar.right, y: info.rcScrollBar.bottom };
                    if ScreenToClient(window, &mut origin) != 0 && ScreenToClient(window, &mut end) != 0 {
                        let bar = RECT { left: origin.x, top: origin.y, right: end.x, bottom: end.y };
                        let arrow = info.dxyLineButton.max(0).min((bar.bottom - bar.top).max(0) / 2);
                        let top_arrow = RECT { left: bar.left, top: bar.top, right: bar.right, bottom: bar.top + arrow };
                        let bottom_arrow = RECT { left: bar.left, top: bar.bottom - arrow, right: bar.right, bottom: bar.bottom };
                        let pen = CreatePen(PS_SOLID, 1, MUTED);
                        if !pen.is_null() {
                            let old = SelectObject(dc, pen);
                            if let Some(rect) = intersection(top_arrow, client) {
                                draw_scrollbar_chevron(dc, rect, true);
                            }
                            if let Some(rect) = intersection(bottom_arrow, client) {
                                draw_scrollbar_chevron(dc, rect, false);
                            }
                            SelectObject(dc, old);
                            DeleteObject(pen);
                        }
                        if info.xyThumbBottom > info.xyThumbTop {
                            let top = (bar.top + info.xyThumbTop).clamp(client.top, client.bottom);
                            let bottom = (bar.top + info.xyThumbBottom).clamp(top, client.bottom);
                            let width = (bar.right - bar.left).max(1);
                            let inset = scaled(1, state.dpi).min(width.saturating_sub(1) / 2);
                            let thumb = RECT { left: (bar.left + inset).max(client.left), top,
                                right: (bar.right - inset).min(client.right), bottom };
                            if thumb.left < thumb.right && thumb.top < thumb.bottom {
                                fill(dc, thumb, if GetCapture() == window { resources.palette.teal } else { resources.palette.muted });
                            }
                        }
                    }
                }
            }
        }
    }
    if message == WM_PAINT { EndPaint(window, &paint); }
}

pub(super) unsafe extern "system" fn scrollbar_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, data: usize) -> LRESULT {
    track_hover(window, message);
    if message == WM_NCDESTROY { RemoveWindowSubclass(window, Some(scrollbar_procedure), 1); }
    if message == WM_NCHITTEST {
        let point = mouse_point(lparam);
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(window, &mut rect) != 0 && point.x >= rect.left && point.x < rect.right
            && point.y >= rect.top && point.y < rect.bottom { return HTCLIENT as LRESULT; }
        if edge_hit_transparent(window, point) { return HTTRANSPARENT as LRESULT; }
    }
    if matches!(message, WM_PAINT | WM_PRINTCLIENT) {
        paint_scrollbar(window, &*(data as *const State), message, wparam);
        return 0;
    }
    if message == WM_ERASEBKGND { return 1; }
    let result = DefSubclassProc(window, message, wparam, lparam);
    if matches!(message, WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MOUSEMOVE | WM_MOUSELEAVE | 0x0118
        | WM_CAPTURECHANGED | WM_CANCELMODE | WM_SETFOCUS | WM_KILLFOCUS) {
        InvalidateRect(window, null(), 0);
        UpdateWindow(window);
    }
    result
}

pub(super) struct Scrolling {
    pub(super) control: HWND,
    pub(super) position: i32,
    pub(super) content: i32,
    pub(super) page: i32,
    pub(super) idle: Option<Instant>,
    pub(super) width: i32,
    pub(super) wheel_remainder: i32,
}
