use super::runtime::State;
use crate::notification_icon::{self, TrayIcon, WM_TRAY_ICON};
use std::io;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

#[derive(Default)]
pub(super) struct NotificationIcon {
    tray: Option<TrayIcon>,
    taskbar_created: u32,
}

impl NotificationIcon {
    pub(super) unsafe fn initialize(&mut self) -> io::Result<()> {
        self.taskbar_created = notification_icon::restart_message()?;
        Ok(())
    }

    pub(super) fn install(&mut self, tray: Option<TrayIcon>) {
        self.tray = tray;
    }

    pub(super) fn remove(&mut self) {
        self.tray = None;
    }
}

pub(super) unsafe fn dispatch_notification_icon(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, state: &mut State) -> Option<LRESULT> {
    if message == state.notification_icon.taskbar_created {
        if let Some(icon) = &mut state.notification_icon.tray {
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
