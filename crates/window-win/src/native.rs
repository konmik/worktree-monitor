use std::io;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{CreateFontW, DeleteObject, HFONT, COLOR_BTNFACE, DEFAULT_CHARSET, FF_MODERN, FIXED_PITCH};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use wts_core::{Monitor, MonitorUpdate, Service};

struct State {
    monitor: Monitor,
    edit: HWND,
    status: HWND,
    font: HFONT,
    latest: Option<Arc<MonitorUpdate>>,
    failure: Option<io::Error>,
}

impl Drop for State {
    fn drop(&mut self) {
        let _ = self.monitor.stop();
        if !self.font.is_null() { unsafe { DeleteObject(self.font); } }
    }
}

fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() }

unsafe fn set_text(window: HWND, text: &str) {
    SetWindowTextW(window, wide(text).as_ptr());
}

unsafe fn font(window: HWND, state: &mut State) -> bool {
    let dpi = GetDpiForWindow(window).max(96);
    let created = CreateFontW(-(14 * dpi as i32 / 96), 0, 0, 0, 400, 0, 0, 0,
        DEFAULT_CHARSET as u32, 0, 0, 0, (FIXED_PITCH | FF_MODERN) as u32, wide("Consolas").as_ptr());
    if created.is_null() { return false; }
    SendMessageW(state.edit, WM_SETFONT, created as WPARAM, 1);
    SendMessageW(state.status, WM_SETFONT, created as WPARAM, 1);
    let previous = std::mem::replace(&mut state.font, created);
    if !previous.is_null() { DeleteObject(previous); }
    true
}

unsafe fn layout(window: HWND, state: &State) {
    let mut client: RECT = std::mem::zeroed();
    if GetClientRect(window, &mut client) == 0 { return; }
    let scale = GetDpiForWindow(window).max(96) as i32;
    let margin = 8 * scale / 96;
    let status_height = 24 * scale / 96;
    let width = (client.right - 2 * margin).max(1);
    let height = (client.bottom - 3 * margin - status_height).max(1);
    MoveWindow(state.edit, margin, margin, width, height, 1);
    MoveWindow(state.status, margin, margin * 2 + height, width, status_height, 1);
}

unsafe fn controls(window: HWND, state: &mut State) -> bool {
    let instance = GetModuleHandleW(null());
    state.edit = CreateWindowExW(WS_EX_CLIENTEDGE, wide("EDIT").as_ptr(), wide("Scanning repositories...").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_HSCROLL | ES_MULTILINE as u32 | ES_READONLY as u32 |
        ES_AUTOVSCROLL as u32 | ES_AUTOHSCROLL as u32, 0, 0, 1, 1, window, null_mut(), instance, null());
    state.status = CreateWindowExW(0, wide("STATIC").as_ptr(), wide("Scanning...").as_ptr(),
        WS_CHILD | WS_VISIBLE, 0, 0, 1, 1, window, null_mut(), instance, null());
    if state.edit.is_null() || state.status.is_null() { return false; }
    SendMessageW(state.edit, 0x00C5, 0x7FFFFFFE, 0);
    if !font(window, state) { return false; }
    layout(window, state);
    SetTimer(window, 1, 200, None) != 0
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut State;
    if pointer.is_null() { return DefWindowProcW(window, message, wparam, lparam); }
    match message {
        WM_CREATE => {
            let state = &mut *pointer;
            if controls(window, state) { 0 } else {
                state.failure = Some(io::Error::last_os_error());
                -1
            }
        }
        WM_SIZE => { layout(window, &*pointer); 0 }
        WM_DPICHANGED => {
            let suggested = &*(lparam as *const RECT);
            SetWindowPos(window, null_mut(), suggested.left, suggested.top,
                suggested.right - suggested.left, suggested.bottom - suggested.top, SWP_NOZORDER | SWP_NOACTIVATE);
            let state = &mut *pointer;
            if !font(window, state) { state.failure = Some(io::Error::last_os_error()); }
            layout(window, state);
            0
        }
        WM_TIMER => {
            let state = &mut *pointer;
            if let Some(update) = state.monitor.latest() {
                if state.latest.as_ref().is_none_or(|old| !Arc::ptr_eq(old, &update)) {
                    if state.latest.as_ref().is_none_or(|old| old.snapshot != update.snapshot) {
                        set_text(state.edit, &update.snapshot.text().replace('\n', "\r\n"));
                    }
                    set_text(state.status, &format!("Polling interval {}s", update.interval_seconds));
                    state.latest = Some(update);
                }
            }
            0
        }
        WM_CLOSE => {
            let state = &mut *pointer;
            KillTimer(window, 1);
            if let Err(error) = state.monitor.stop() { state.failure = Some(io::Error::other(error)); }
            DestroyWindow(window);
            0
        }
        WM_DESTROY => { PostQuitMessage(0); 0 }
        WM_NCDESTROY => {
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            DefWindowProcW(window, message, wparam, lparam)
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

pub fn run(service: Service) -> Result<(), io::Error> {
    let mut state = Box::new(State {
        monitor: service.monitor().map_err(io::Error::other)?, edit: null_mut(), status: null_mut(),
        font: null_mut(), latest: None, failure: None,
    });
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(null());
        if instance.is_null() { return Err(io::Error::last_os_error()); }
        let class = wide("WtsStatusWindow");
        let definition = WNDCLASSW {
            lpfnWndProc: Some(procedure), hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW), hbrBackground: (COLOR_BTNFACE + 1) as _,
            lpszClassName: class.as_ptr(), ..std::mem::zeroed()
        };
        if RegisterClassW(&definition) == 0 { return Err(io::Error::last_os_error()); }
        let window = CreateWindowExW(WS_EX_TOPMOST, class.as_ptr(), wide("WTS Worktree status").as_ptr(),
            WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT, 600, 400, null_mut(), null_mut(), instance,
            (&mut *state as *mut State).cast());
        if window.is_null() {
            let error = state.failure.take().unwrap_or_else(io::Error::last_os_error);
            UnregisterClassW(class.as_ptr(), instance);
            return Err(error);
        }
        ShowWindow(window, SW_SHOWNOACTIVATE);
        let dpi = GetDpiForWindow(window).max(96) as i32;
        if SetWindowPos(window, HWND_TOPMOST, 0, 0, 600 * dpi / 96, 400 * dpi / 96, SWP_NOMOVE | SWP_NOACTIVATE) == 0 {
            state.failure = Some(io::Error::last_os_error());
            DestroyWindow(window);
        }
        let mut message: MSG = std::mem::zeroed();
        loop {
            let received = GetMessageW(&mut message, null_mut(), 0, 0);
            if received == -1 {
                state.failure = Some(io::Error::last_os_error());
                DestroyWindow(window);
                break;
            }
            if received == 0 { break; }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let stopped = state.monitor.stop().map_err(io::Error::other);
        UnregisterClassW(class.as_ptr(), instance);
        stopped?;
        if let Some(error) = state.failure.take() { return Err(error); }
    }
    Ok(())
}
