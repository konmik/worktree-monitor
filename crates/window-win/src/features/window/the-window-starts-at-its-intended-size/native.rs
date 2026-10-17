use std::ptr::null_mut;
use super::font_resources::fonts;
use super::row_layout::layout;
use super::row_ordering::cancel_drag;
use super::runtime::State;
use super::scroll_anchor::anchor;
use std::io;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITORINFO, MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};

pub(in crate::native) unsafe fn reset_window_bounds(window: HWND) -> io::Result<()> {
    let dpi = GetDpiForWindow(window).max(96) as i32;
    let mut monitor: MONITORINFO = std::mem::zeroed();
    monitor.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST), &mut monitor) == 0 {
        return Err(io::Error::last_os_error());
    }
    let work = monitor.rcWork;
    let width = (DEFAULT_WIDTH * dpi / 96).min(work.right - work.left);
    let height = (DEFAULT_HEIGHT * dpi / 96).min(work.bottom - work.top);
    if SetWindowPos(window, HWND_TOPMOST, work.left, work.top, width, height, SWP_NOACTIVATE) == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) const DEFAULT_WIDTH: i32 = 640;
pub(super) const DEFAULT_HEIGHT: i32 = 540;

pub(super) unsafe fn update_dpi(window: HWND, pointer: *mut State, lparam: LPARAM) -> LRESULT {
    cancel_drag(pointer);
    let previous_dpi = (*pointer).dpi;
    let saved = anchor(&*pointer).map(|(key, offset)| (key,
        (i64::from(offset) * i64::from(GetDpiForWindow(window).max(96)) / i64::from((*pointer).dpi)) as i32));
    if !fonts(window, &mut *pointer) { (*pointer).failure = Some(io::Error::last_os_error()); }
    if !(*pointer).starting && (*pointer).dpi != previous_dpi {
        let suggested = &*(lparam as *const RECT);
        SetWindowPos(window, null_mut(), suggested.left, suggested.top, suggested.right - suggested.left,
            suggested.bottom - suggested.top, SWP_NOZORDER | SWP_NOACTIVATE);
    }
    layout(window, &mut *pointer, saved, true);
    0
}
