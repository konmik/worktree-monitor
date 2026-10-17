use super::*;

pub(super) unsafe fn reset_window_bounds(window: HWND) -> io::Result<()> {
    let dpi = GetDpiForWindow(window).max(96) as i32;
    let mut monitor: MONITORINFO = std::mem::zeroed();
    monitor.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST), &mut monitor) == 0 {
        return Err(io::Error::last_os_error());
    }
    let work = monitor.rcWork;
    let width = (640 * dpi / 96).min(work.right - work.left);
    let height = (540 * dpi / 96).min(work.bottom - work.top);
    if SetWindowPos(window, HWND_TOPMOST, work.left, work.top, width, height, SWP_NOACTIVATE) == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

