use super::row_ordering::cancel_drag;
use super::runtime::State;
use crate::placement_win;
use std::io;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{HWND, LRESULT};
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyWindow, KillTimer, PostQuitMessage};

impl Drop for State {
    fn drop(&mut self) { let _ = self.monitor.stop(); }
}

impl State {
    pub(super) fn stop_monitor(&mut self) -> io::Result<()> {
        self.monitor.stop().map_err(io::Error::other)
    }
}

pub(super) unsafe fn close(window: HWND, pointer: *mut State) -> LRESULT {
    cancel_drag(pointer);
    let overlay = std::mem::replace(&mut (*pointer).indicator_overlay, null_mut());
    if !overlay.is_null() { DestroyWindow(overlay); }
    let _ = placement_win::save(window, &(*pointer).placement_path);
    KillTimer(window, 1);
    if let Err(error) = (*pointer).stop_monitor() { (*pointer).failure = Some(error); }
    DestroyWindow(window);
    0
}

pub(super) unsafe fn destroy(window: HWND, pointer: *mut State) -> LRESULT {
    (*pointer).tray = None;
    cancel_drag(pointer);
    (*pointer).indicator_overlay = null_mut();
    KillTimer(window, 1);
    PostQuitMessage(0);
    0
}
