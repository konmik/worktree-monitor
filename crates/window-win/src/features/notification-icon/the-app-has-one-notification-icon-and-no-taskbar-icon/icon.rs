use crate::context_menu;
use std::io;
use std::ptr::null;
use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::Shell::{Shell_NotifyIconW, NOTIFYICONDATAW, NIF_MESSAGE, NIF_ICON, NIF_TIP, NIF_SHOWTIP,
    NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT, NINF_KEY, NOTIFYICON_VERSION_4};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub(crate) const WM_TRAY_ICON: u32 = WM_APP + 43;
pub(crate) const WINDOW_STYLE: u32 = WS_EX_TOOLWINDOW;
const APP_ICON: *const u16 = std::ptr::without_provenance(1);

pub(crate) unsafe fn restart_message() -> io::Result<u32> {
    let message = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
    if message == 0 { Err(io::Error::last_os_error()) } else { Ok(message) }
}

pub(crate) unsafe fn callback(window: HWND, wparam: WPARAM, lparam: LPARAM) {
    if lparam as u32 >> 16 != 1 { return; }
    match lparam as u32 & 0xffff {
        NIN_SELECT => show_from_tray(window),
        event if event == NIN_SELECT | NINF_KEY => show_from_tray(window),
        WM_CONTEXTMENU => {
            context_menu::request(window, wparam as LPARAM);
        }
        _ => {}
    }
}

fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() }

pub(crate) struct TrayIcon {
    data: NOTIFYICONDATAW,
    added: bool,
}

impl TrayIcon {
    pub(crate) unsafe fn restore(&mut self) -> io::Result<()> {
        self.added = false;
        self.add()
    }

    pub(crate) unsafe fn new(window: HWND) -> Result<Self, io::Error> {
        let mut data: NOTIFYICONDATAW = std::mem::zeroed();
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = window;
        data.uID = 1;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        data.uCallbackMessage = WM_TRAY_ICON;
        let dpi = GetDpiForWindow(window).max(96);
        data.hIcon = LoadImageW(GetModuleHandleW(null()), APP_ICON, IMAGE_ICON,
            GetSystemMetricsForDpi(SM_CXSMICON, dpi), GetSystemMetricsForDpi(SM_CYSMICON, dpi), 0).cast();
        if data.hIcon.is_null() { return Err(io::Error::last_os_error()); }
        data.szTip[..4].copy_from_slice(&wide("WTM"));
        let mut icon = Self { data, added: false };
        icon.add()?;
        Ok(icon)
    }

    unsafe fn add(&mut self) -> Result<(), io::Error> {
        if self.added { return Ok(()); }
        if Shell_NotifyIconW(NIM_ADD, &self.data) == 0 && Shell_NotifyIconW(NIM_MODIFY, &self.data) == 0 {
            return Err(io::Error::other("Cannot add the WTM notification icon"));
        }
        self.added = true;
        self.data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        if Shell_NotifyIconW(NIM_SETVERSION, &self.data) == 0 {
            return Err(io::Error::other("Cannot set the WTM notification icon version"));
        }
        Ok(())
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            if self.added { Shell_NotifyIconW(NIM_DELETE, &self.data); }
            DestroyIcon(self.data.hIcon);
        }
    }
}

pub(crate) unsafe fn show_from_tray(window: HWND) {
    ShowWindow(window, if IsIconic(window) != 0 { SW_RESTORE } else { SW_SHOWNOACTIVATE });
    SetForegroundWindow(window);
}

