use crate::opacity::Opacity;
use std::io;
use std::ptr::null;
use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[path = "users-can-select-opacity-in-five-percent-steps/menu.rs"]
mod opacity;

#[path = "users-can-hide-or-show-the-window/menu.rs"]
mod visibility;
pub(crate) use visibility::show_window;

#[path = "reset-position-returns-the-window-to-defaults/menu.rs"]
mod reset_position;

#[path = "users-can-exit-the-app/menu.rs"]
mod exit;

struct Menu(HMENU);

pub(crate) enum Action {
    Hide,
    Show,
    Exit,
    ResetPosition,
    Opacity(Opacity),
    None,
}

impl Menu {
    unsafe fn new(visible: bool, opacity: Opacity) -> io::Result<Self> {
        let menu = CreatePopupMenu();
        if menu.is_null() { return Err(io::Error::last_os_error()); }
        let menu = Self(menu);
        opacity::append(menu.0, opacity)?;
        visibility::append(menu.0, visible)?;
        reset_position::append(menu.0)?;
        exit::append(menu.0)?;
        Ok(menu)
    }
}

impl Drop for Menu {
    fn drop(&mut self) { unsafe { DestroyMenu(self.0); } }
}

pub(crate) unsafe fn request(window: HWND, position: LPARAM) {
    SendMessageW(window, WM_CONTEXTMENU, window as WPARAM, position);
}

pub(crate) unsafe fn show(window: HWND, point: POINT, opacity: Opacity) -> io::Result<Action> {
    let visible = IsWindowVisible(window) != 0 && IsIconic(window) == 0;
    let menu = Menu::new(visible, opacity)?;
    SetForegroundWindow(window);
    let command = TrackPopupMenu(menu.0, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
        point.x, point.y, 0, window, null());
    Ok(match command {
        command if command == visibility::COMMAND as i32 => visibility::action(visible),
        command if command == exit::COMMAND as i32 => Action::Exit,
        command if command == reset_position::COMMAND as i32 => Action::ResetPosition,
        command => opacity::action(command),
    })
}

fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() }

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
