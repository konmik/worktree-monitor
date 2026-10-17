use super::runtime::{State, wide};
use super::window_appearance::Palette;
use super::cell_fonts::CellFonts;
use crate::typography::{FONT_SIZE, Typography};
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{HWND, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{CLEARTYPE_QUALITY, CreateCompatibleDC, CreateFontW, DEFAULT_CHARSET, DeleteDC, DeleteObject, FF_MODERN, FIXED_PITCH, GetTextExtentPoint32W, GetTextFaceW, GetTextMetricsW, HDC, HFONT, RestoreDC, SaveDC, SelectObject, TEXTMETRICW, TMPF_FIXED_PITCH};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_SETFONT};
pub(super) unsafe fn fonts(window: HWND, state: &mut State) -> bool {
    state.indicators_dirty = true;
    let dpi = GetDpiForWindow(window).max(96) as i32;
    let Some(resources) = Resources::new(dpi) else { return false; };
    for card in state.fixed.iter().chain(&state.cards) {
        SendMessageW(card.window, WM_SETFONT, resources.font as WPARAM, 0);
        if let Some(action) = card.action {
            SendMessageW(action, WM_SETFONT, resources.font as WPARAM, 0);
        }
    }
    if !state.show_all_button.is_null() {
        SendMessageW(state.show_all_button, WM_SETFONT, resources.font as WPARAM, 0);
    }
    state.resources = Some(resources);
    state.dpi = dpi;
    true
}

pub(super) unsafe fn wrapped_width(dc: HDC, value: &str, font: HFONT) -> i32 {
    let saved = SaveDC(dc);
    SelectObject(dc, font);
    let mut size = std::mem::zeroed();
    let value = wide(value);
    GetTextExtentPoint32W(dc, value.as_ptr(), (value.len() - 1) as i32, &mut size);
    RestoreDC(dc, saved);
    size.cx
}

pub(super) struct Resources {
    pub(super) font: HFONT,
    pub(super) cell_fonts: CellFonts,
    pub(super) typography: Typography,
    pub(super) space_width: i32,
    pub(super) palette: Palette,
}

impl Resources {
    pub(super) unsafe fn new(dpi: i32) -> Option<Self> {
        let mut resources = Self { font: null_mut(), cell_fonts: CellFonts::empty(), typography: Typography::new(dpi, 1), space_width: 1, palette: Palette::new() };
        let dc = CreateCompatibleDC(null_mut());
        if dc.is_null() { return None; }
        for face in ["Berkeley Mono", "Berkeley Mono Variable", "JetBrains Mono", "Consolas", "Cascadia Mono", "Courier New", ""] {
            let font = CreateFontW(-(FONT_SIZE * dpi / 96), 0, 0, 0, 400, 0, 0, 0,
                DEFAULT_CHARSET as u32, 0, 0, CLEARTYPE_QUALITY as u32, (FIXED_PITCH | FF_MODERN) as u32,
                wide(face).as_ptr());
            if font.is_null() { continue; }
            let old = SelectObject(dc, font);
            let mut actual = [0u16; 64];
            let length = GetTextFaceW(dc, actual.len() as i32, actual.as_mut_ptr()).max(0) as usize;
            let mut metrics: TEXTMETRICW = std::mem::zeroed();
            let monospaced = GetTextMetricsW(dc, &mut metrics) != 0 && metrics.tmPitchAndFamily & TMPF_FIXED_PITCH == 0;
            SelectObject(dc, old);
            let actual = String::from_utf16_lossy(&actual[..length.min(actual.len())]);
            if (face.is_empty() || actual.trim_end_matches('\0').eq_ignore_ascii_case(face)) && monospaced {
                resources.font = font;
                resources.typography = Typography::new(dpi, metrics.tmHeight);
                resources.space_width = metrics.tmAveCharWidth;
                break;
            }
            DeleteObject(font);
        }
        DeleteDC(dc);
        if resources.font.is_null() || !resources.palette.is_valid() {
            None
        } else {
            resources.cell_fonts = CellFonts::new(resources.font, resources.space_width)?;
            Some(resources)
        }
    }
}

impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            if !self.font.is_null() { DeleteObject(self.font); }
        }
    }
}

