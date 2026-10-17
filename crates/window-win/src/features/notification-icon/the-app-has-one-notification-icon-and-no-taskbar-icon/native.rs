use super::runtime::State;
use crate::notification_icon;
use crate::notification_icon::WM_TRAY_ICON;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

pub(super) unsafe fn dispatch_notification_icon(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, state: &mut State) -> Option<LRESULT> {
    if message == state.taskbar_created {
        if let Some(icon) = &mut state.tray {
            if let Err(error) = icon.restore() {
                state.failure = Some(error);
                PostMessageW(window, WM_CLOSE, 0, 0);
            }
        }
        Some(0)
    } else if message == WM_TRAY_ICON {
        notification_icon::callback(window, wparam, lparam);
        Some(0)
    } else { None }
}
