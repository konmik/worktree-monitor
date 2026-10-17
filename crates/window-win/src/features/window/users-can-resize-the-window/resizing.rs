
use super::runtime::scaled;
use windows_sys::Win32::Foundation::{HWND, LRESULT, POINT, RECT};
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IsZoomed, SM_CXPADDEDBORDER, SM_CXSIZEFRAME, SM_CYSIZEFRAME};
pub(super) unsafe fn resize_hit(window: HWND, point: POINT) -> Option<LRESULT> {
    if IsZoomed(window) != 0 { return None; }
    let mut rect: RECT = std::mem::zeroed();
    if GetWindowRect(window, &mut rect) == 0 || point.x < rect.left || point.x >= rect.right
        || point.y < rect.top || point.y >= rect.bottom { return None; }
    let dpi = GetDpiForWindow(window).max(96) as i32;
    let frame_x = (GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi as u32)
        + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi as u32)).max(scaled(4, dpi));
    let frame_y = (GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi as u32)
        + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi as u32)).max(scaled(4, dpi));
    let left = point.x < rect.left + frame_x;
    let right = point.x >= rect.right - frame_x;
    let top = point.y < rect.top + frame_y;
    let bottom = point.y >= rect.bottom - frame_y;
    Some(match (left, right, top, bottom) {
        (true, _, true, _) => HTTOPLEFT as LRESULT,
        (_, true, true, _) => HTTOPRIGHT as LRESULT,
        (true, _, _, true) => HTBOTTOMLEFT as LRESULT,
        (_, true, _, true) => HTBOTTOMRIGHT as LRESULT,
        (_, _, true, _) => HTTOP as LRESULT,
        (_, _, _, true) => HTBOTTOM as LRESULT,
        (true, _, _, _) => HTLEFT as LRESULT,
        (_, true, _, _) => HTRIGHT as LRESULT,
        _ => return None,
    })
}
