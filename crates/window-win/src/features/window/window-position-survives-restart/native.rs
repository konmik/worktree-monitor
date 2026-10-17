use super::initial_geometry::reset_window_bounds;
use crate::placement::ShowState;
use crate::placement_win;
use std::io;
use std::path::Path;
use windows_sys::Win32::Foundation::HWND;

pub(super) unsafe fn restore_window_geometry(window: HWND, path: &Path) -> (Option<ShowState>, Result<(), io::Error>) {
    let restored = placement_win::restore_hidden_geometry(window, path).unwrap_or(None);
    let positioned = if restored.is_none() { reset_window_bounds(window) } else { Ok(()) };
    (restored, positioned)
}
