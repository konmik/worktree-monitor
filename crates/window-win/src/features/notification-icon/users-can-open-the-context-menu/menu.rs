use crate::opacity::Opacity;
use std::io;
use std::ptr::null;
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

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
        let opacity_menu = CreatePopupMenu();
        if opacity_menu.is_null() {
            let error = io::Error::last_os_error();
            DestroyMenu(menu);
            return Err(error);
        }
        let populated = (5..=100).step_by(5).all(|percent| {
            AppendMenuW(opacity_menu, MF_STRING, percent, wide(&format!("{percent}%")).as_ptr()) != 0
        }) && CheckMenuRadioItem(opacity_menu, 5, 100, u32::from(opacity.percent()), MF_BYCOMMAND) != 0;
        if !populated || AppendMenuW(menu, MF_POPUP, opacity_menu as usize, wide("Opacity").as_ptr()) == 0 {
            let error = io::Error::last_os_error();
            DestroyMenu(opacity_menu);
            DestroyMenu(menu);
            return Err(error);
        }
        let populated = AppendMenuW(menu, MF_STRING, 1, wide(if visible { "Hide" } else { "Show" }).as_ptr()) != 0
            && AppendMenuW(menu, MF_STRING, 3, wide("Reset position").as_ptr()) != 0
            && AppendMenuW(menu, MF_STRING, 2, wide("Exit").as_ptr()) != 0;
        if !populated {
            let error = io::Error::last_os_error();
            DestroyMenu(menu);
            return Err(error);
        }
        Ok(Self(menu))
    }
}

impl Drop for Menu {
    fn drop(&mut self) { unsafe { DestroyMenu(self.0); } }
}

pub(crate) unsafe fn show(window: HWND, point: POINT, opacity: Opacity) -> io::Result<Action> {
    let visible = IsWindowVisible(window) != 0 && IsIconic(window) == 0;
    let menu = Menu::new(visible, opacity)?;
    SetForegroundWindow(window);
    let command = TrackPopupMenu(menu.0, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
        point.x, point.y, 0, window, null());
    Ok(match command {
        1 if visible => Action::Hide,
        1 => Action::Show,
        2 => Action::Exit,
        3 => Action::ResetPosition,
        command => u8::try_from(command).ok().and_then(Opacity::new).map(Action::Opacity).unwrap_or(Action::None),
    })
}

fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() }

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
