use crate::placement::{self, Monitor, Placement, Rect, RestorePlacement, ShowState};
use std::io;
use std::mem::size_of;
use std::path::Path;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, MonitorFromRect, MonitorFromWindow,
    HMONITOR, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GetWindowPlacement, GetWindowRect, IsIconic,
    SetWindowLongPtrW, SetWindowPlacement, GWL_EXSTYLE, GWL_STYLE, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED,
    SW_SHOWNOACTIVATE, WINDOWPLACEMENT, WPF_RESTORETOMAXIMIZED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_MAXIMIZE};

struct MonitorList {
    monitors: Vec<Monitor>,
    error: Option<io::Error>,
}

pub(crate) unsafe fn restore(hwnd: HWND, path: &Path) -> io::Result<bool> {
    let Some(saved) = placement::read(path)? else { return Ok(false); };
    let monitors = monitors()?;
    let Some(current) = monitors.iter().find(|monitor| monitor.identity == saved.monitor.identity) else {
        return Ok(false);
    };
    let Some(candidate) = saved.restore_for(current) else { return Ok(false); };
    let placement = native_placement(candidate, current, is_tool_window(hwnd))?;
    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE as isize);
    let result = SetWindowPlacement(hwnd, &placement);
    let error = if result == 0 { Some(io::Error::last_os_error()) } else { None };
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
    if let Some(error) = error {
        return Err(error);
    }
    Ok(true)
}

pub(crate) unsafe fn save(hwnd: HWND, path: &Path) -> io::Result<()> {
    let mut native = WINDOWPLACEMENT { length: size_of::<WINDOWPLACEMENT>() as u32, ..std::mem::zeroed() };
    if GetWindowPlacement(hwnd, &mut native) == 0 {
        return Err(io::Error::last_os_error());
    }
    let minimized = IsIconic(hwnd) != 0 || native.showCmd == SW_SHOWMINIMIZED as u32;
    let maximized = native.showCmd == SW_SHOWMAXIMIZED as u32
        || (minimized && (native.flags & WPF_RESTORETOMAXIMIZED != 0
            || (GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_MAXIMIZE != 0)));
    let base = monitor_from_window(hwnd)?;
    let (normal, native_normal) = if minimized || maximized {
        let native_normal = workspace_to_screen(native.rcNormalPosition, &base, is_tool_window(hwnd))?;
        (model_rect(native_normal)?, native_normal)
    } else {
        let mut native_normal: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut native_normal) == 0 {
            return Err(io::Error::last_os_error());
        }
        (model_rect(native_normal)?, native_normal)
    };
    let handle = MonitorFromRect(&native_normal, MONITOR_DEFAULTTONEAREST);
    let monitor = if handle.is_null() { base } else { monitor_info(handle)? };
    let placement = Placement::new(normal, monitor, if maximized { ShowState::Maximized } else { ShowState::Normal });
    placement::write(path, &placement)
}

unsafe fn native_placement(candidate: RestorePlacement, monitor: &Monitor, tool_window: bool) -> io::Result<WINDOWPLACEMENT> {
    let normal = screen_to_workspace(candidate.normal, monitor, tool_window)?;
    Ok(WINDOWPLACEMENT {
        length: size_of::<WINDOWPLACEMENT>() as u32,
        showCmd: match candidate.state {
            ShowState::Normal => SW_SHOWNOACTIVATE as u32,
            ShowState::Maximized => SW_SHOWMAXIMIZED as u32,
        },
        ptMinPosition: POINT { x: 0, y: 0 },
        ptMaxPosition: POINT { x: 0, y: 0 },
        rcNormalPosition: normal,
        ..std::mem::zeroed()
    })
}

unsafe fn monitor_from_window(hwnd: HWND) -> io::Result<Monitor> {
    let handle = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    if handle.is_null() {
        Err(io::Error::last_os_error())
    } else {
        monitor_info(handle)
    }
}

unsafe fn monitors() -> io::Result<Vec<Monitor>> {
    let mut list = MonitorList { monitors: Vec::new(), error: None };
    if EnumDisplayMonitors(null_mut(), null(), Some(enum_monitor), (&mut list as *mut MonitorList) as LPARAM) == 0 {
        if let Some(error) = list.error { return Err(error); }
        return Err(io::Error::last_os_error());
    }
    if let Some(error) = list.error { return Err(error); }
    if list.monitors.is_empty() { return Err(invalid("No display monitors found")); }
    Ok(list.monitors)
}

unsafe extern "system" fn enum_monitor(handle: HMONITOR, _dc: windows_sys::Win32::Graphics::Gdi::HDC,
    _clip: *mut RECT, data: LPARAM) -> windows_sys::core::BOOL {
    let list = &mut *(data as *mut MonitorList);
    match monitor_info(handle) {
        Ok(monitor) => list.monitors.push(monitor),
        Err(error) => { list.error = Some(error); return 0; }
    }
    1
}

unsafe fn monitor_info(handle: HMONITOR) -> io::Result<Monitor> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(handle, &mut info.monitorInfo) == 0 {
        return Err(io::Error::last_os_error());
    }
    let end = info.szDevice.iter().position(|value| *value == 0).unwrap_or(info.szDevice.len());
    let identity = String::from_utf16(&info.szDevice[..end]).map_err(|_| invalid("Invalid monitor identity"))?;
    let full_bounds = model_rect(info.monitorInfo.rcMonitor)?;
    let work_area = model_rect(info.monitorInfo.rcWork)?;
    let geometry = Monitor::new(identity, full_bounds, work_area);
    if !geometry.full_bounds.is_valid() || !geometry.work_area.fits_within(geometry.full_bounds) {
        return Err(invalid("Invalid monitor geometry"));
    }
    Ok(geometry)
}

fn model_rect(rect: RECT) -> io::Result<Rect> {
    let result = Rect::new(rect.left, rect.top, rect.right, rect.bottom);
    if result.is_valid() { Ok(result) } else { Err(invalid("Invalid window rectangle")) }
}

fn workspace_to_screen(rect: RECT, monitor: &Monitor, tool_window: bool) -> io::Result<RECT> {
    if !tool_window {
        let x = monitor.work_area.left.checked_sub(monitor.full_bounds.left).ok_or_else(|| invalid("Monitor offset overflow"))?;
        let y = monitor.work_area.top.checked_sub(monitor.full_bounds.top).ok_or_else(|| invalid("Monitor offset overflow"))?;
        Ok(RECT { left: rect.left.checked_add(x).ok_or_else(|| invalid("Window rectangle overflow"))?,
            top: rect.top.checked_add(y).ok_or_else(|| invalid("Window rectangle overflow"))?,
            right: rect.right.checked_add(x).ok_or_else(|| invalid("Window rectangle overflow"))?,
            bottom: rect.bottom.checked_add(y).ok_or_else(|| invalid("Window rectangle overflow"))? })
    } else {
        Ok(rect)
    }
}

fn screen_to_workspace(rect: Rect, monitor: &Monitor, tool_window: bool) -> io::Result<RECT> {
    if !tool_window {
        let x = monitor.work_area.left.checked_sub(monitor.full_bounds.left).ok_or_else(|| invalid("Monitor offset overflow"))?;
        let y = monitor.work_area.top.checked_sub(monitor.full_bounds.top).ok_or_else(|| invalid("Monitor offset overflow"))?;
        Ok(RECT { left: rect.left.checked_sub(x).ok_or_else(|| invalid("Window rectangle overflow"))?,
            top: rect.top.checked_sub(y).ok_or_else(|| invalid("Window rectangle overflow"))?,
            right: rect.right.checked_sub(x).ok_or_else(|| invalid("Window rectangle overflow"))?,
            bottom: rect.bottom.checked_sub(y).ok_or_else(|| invalid("Window rectangle overflow"))? })
    } else {
        Ok(RECT { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom })
    }
}

unsafe fn is_tool_window(hwnd: HWND) -> bool {
    GetWindowLongPtrW(hwnd, windows_sys::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0
}

fn invalid(message: impl Into<String>) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, message.into()) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_conversion_uses_taskbar_offsets_not_monitor_origins() {
        let monitor = Monitor::new("secondary", Rect::new(-1920, -120, 0, 1080), Rect::new(-1880, -80, 0, 1080));
        let normal = Rect::new(-1800, -60, -1200, 540);
        let workspace = screen_to_workspace(normal, &monitor, false).unwrap();
        assert_eq!((workspace.left, workspace.top, workspace.right, workspace.bottom), (-1840, -100, -1240, 500));
        assert_eq!(model_rect(workspace_to_screen(workspace, &monitor, false).unwrap()).unwrap(), normal);
        let tool = screen_to_workspace(normal, &monitor, true).unwrap();
        assert_eq!((tool.left, tool.top, tool.right, tool.bottom), (-1800, -60, -1200, 540));
    }
}
