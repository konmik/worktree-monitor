use crate::context_menu;

use crate::context_menu::Action;
use std::io;
use super::runtime::State;
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_NULL};
#[path = "users-can-select-opacity-in-five-percent-steps/native.rs"]
pub(super) mod opacity;

#[path = "users-can-hide-or-show-the-window/native.rs"]
pub(super) mod visibility;

#[path = "reset-position-returns-the-window-to-defaults/native.rs"]
pub(super) mod reset_position;

#[path = "users-can-exit-the-app/native.rs"]
pub(super) mod exit;

pub(super) unsafe fn context_menu(window: HWND, point: POINT, pointer: *mut State) -> Result<(), io::Error> {
    match context_menu::show(window, point, (*pointer).opacity)? {
        Action::Hide => visibility::hide(window),
        Action::Show => visibility::show(window),
        Action::Exit => exit::apply(window),
        Action::ResetPosition => reset_position::apply(window, &(*pointer).placement_path)?,
        Action::Opacity(opacity) => opacity::apply(window, &mut *pointer, opacity)?,
        Action::None => {}
    }
    PostMessageW(window, WM_NULL, 0, 0);
    Ok(())
}
