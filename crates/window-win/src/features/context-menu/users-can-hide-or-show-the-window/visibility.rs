
use super::super::context_menu_actions::context_menu;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
pub(super) unsafe fn hide(window: HWND) {
    ShowWindow(window, SW_HIDE);
}

pub(super) unsafe fn show(window: HWND) {
    context_menu::show_window(window);
}
