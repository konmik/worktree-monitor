
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
pub(super) unsafe fn apply(window: HWND) {
    PostMessageW(window, WM_CLOSE, 0, 0);
}
