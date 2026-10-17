use super::runtime::State;
use super::initial_geometry::{DEFAULT_HEIGHT, DEFAULT_WIDTH};
use crate::taskbar_icon;
use crate::placement::ShowState;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{HINSTANCE, HWND};
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, CW_USEDEFAULT, GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, ShowWindow, SW_SHOWMAXIMIZED, SW_SHOWNOACTIVATE, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU, WS_THICKFRAME};
use super::runtime::wide;

pub(super) unsafe fn create(class: *const u16, instance: HINSTANCE, state: &mut State) -> HWND {
    CreateWindowExW(WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED | taskbar_icon::WINDOW_STYLE, class, wide("WTM Worktree status").as_ptr(),
    WS_POPUP | WS_SYSMENU | WS_THICKFRAME | WS_CLIPCHILDREN | WS_CLIPSIBLINGS, CW_USEDEFAULT, CW_USEDEFAULT, DEFAULT_WIDTH, DEFAULT_HEIGHT, null_mut(), null_mut(), instance,
    (state as *mut State).cast())
}

pub(super) unsafe fn show(window: HWND, restored: Option<ShowState>) {
    ShowWindow(window, if restored == Some(ShowState::Maximized) { SW_SHOWMAXIMIZED } else { SW_SHOWNOACTIVATE });
    let style = GetWindowLongPtrW(window, GWL_EXSTYLE);
    SetWindowLongPtrW(window, GWL_EXSTYLE, style & !(WS_EX_NOACTIVATE as isize));
}
