use crate::window::initial_geometry::reset_window_bounds;

use crate::placement_win;
use std::io;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{SW_SHOWNORMAL, SetForegroundWindow, ShowWindow};


pub(super) unsafe fn apply(window: HWND, placement_path: &std::path::Path) -> io::Result<()> {
    ShowWindow(window, SW_SHOWNORMAL);
    reset_window_bounds(window)?;
    SetForegroundWindow(window);
    placement_win::save(window, placement_path)?;
    Ok(())
}
