use wtm_ui::render_buffer::CellFlags;
use std::ptr::null_mut;
use windows_sys::Win32::Graphics::Gdi::{ANTIALIASED_QUALITY, CLEARTYPE_QUALITY, CreateFontIndirectW, DeleteObject, GetObjectW, HFONT, LOGFONTW};

pub(super) struct CellFonts {
    fonts: [HFONT; 8],
}

impl CellFonts {
    pub(super) fn empty() -> Self { Self { fonts: [null_mut(); 8] } }

    pub(super) unsafe fn new(base: HFONT, width: i32) -> Option<Self> {
        let mut fonts = Self::empty();
        let mut font: LOGFONTW = std::mem::zeroed();
        if GetObjectW(base, std::mem::size_of::<LOGFONTW>() as i32, (&mut font as *mut LOGFONTW).cast()) == 0 { return None; }
        font.lfWidth = width;
        for index in 0..8 {
            font.lfWeight = if index & 1 != 0 { 700 } else { 400 };
            font.lfUnderline = u8::from(index & 2 != 0);
            font.lfQuality = if index & 4 != 0 { ANTIALIASED_QUALITY } else { CLEARTYPE_QUALITY };
            fonts.fonts[index] = CreateFontIndirectW(&font);
        }
        if fonts.fonts.iter().any(|font| font.is_null()) { None } else { Some(fonts) }
    }

    pub(super) fn font(&self, base: HFONT, flags: CellFlags, overlay: bool) -> HFONT {
        if !flags.bold && !flags.underline && !overlay { return base; }
        self.fonts[usize::from(flags.bold) | (usize::from(flags.underline) << 1) | (usize::from(overlay) << 2)]
    }
}

impl Drop for CellFonts {
    fn drop(&mut self) {
        unsafe {
            for font in self.fonts { if !font.is_null() { DeleteObject(font); } }
        }
    }
}

#[cfg(test)]
#[path = "fonts.test.rs"]
mod tests;
