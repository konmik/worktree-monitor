use super::runtime::State;
use super::buffer_presentation;
use windows_sys::Win32::Foundation::{COLORREF, HWND};
use windows_sys::Win32::Graphics::Gdi::{BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, HBRUSH, PAINTSTRUCT};
pub(super) const BACKGROUND: COLORREF = rgb(11, 16, 18);
pub(super) const fn rgb(red: u32, green: u32, blue: u32) -> COLORREF { red | (green << 8) | (blue << 16) }

pub(super) unsafe fn paint_background(window: HWND, state: &State) {
    let mut paint: PAINTSTRUCT = std::mem::zeroed();
    let dc = BeginPaint(window, &mut paint);
    buffer_presentation::paint_frame(dc, paint.rcPaint, state);
    EndPaint(window, &paint);
}

use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;

pub(super) unsafe fn configure_window_appearance(window: HWND) {
    let dark = 1i32;
    DwmSetWindowAttribute(window, 20, (&dark as *const i32).cast(), std::mem::size_of::<i32>() as u32);
    let disabled = 1u32;
    DwmSetWindowAttribute(window, 2, (&disabled as *const u32).cast(), std::mem::size_of::<u32>() as u32);
    DwmSetWindowAttribute(window, 33, (&disabled as *const u32).cast(), std::mem::size_of::<u32>() as u32);
    let no_border = 0xfffffffeu32;
    DwmSetWindowAttribute(window, 34, (&no_border as *const u32).cast(), std::mem::size_of::<u32>() as u32);
}

pub(super) struct Palette {
    pub(super) background: HBRUSH,
}

impl Palette {
    pub(super) unsafe fn new() -> Self {
        Self { background: CreateSolidBrush(BACKGROUND) }
    }

    pub(super) fn is_valid(&self) -> bool {
        !self.background.is_null()
    }
}

impl Drop for Palette {
    fn drop(&mut self) {
        unsafe {
            if !self.background.is_null() { DeleteObject(self.background); }
        }
    }
}
