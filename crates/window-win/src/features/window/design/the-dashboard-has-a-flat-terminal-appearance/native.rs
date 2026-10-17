use super::dashboard_rows::{Card, draw_card};
use super::font_resources::{Resources};
use super::row_layout::{wrapped_lines};
use super::runtime::{State, wide};
use super::visibility_controls::{paint_button};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT};
use windows_sys::Win32::Graphics::Gdi::{BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DT_NOPREFIX, DT_SINGLELINE, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, HBRUSH, HDC, HFONT, PAINTSTRUCT, SRCCOPY, SelectObject, SetBkMode, SetTextColor, TRANSPARENT};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODT_BUTTON, ODT_STATIC};
pub(super) const BACKGROUND: COLORREF = rgb(11, 16, 18);
pub(super) const GREEN: COLORREF = rgb(194, 249, 112);
pub(super) const TEAL: COLORREF = rgb(84, 213, 197);
pub(super) const RED: COLORREF = rgb(255, 131, 137);
pub(super) const WHITE: COLORREF = rgb(232, 239, 235);
pub(super) const MUTED: COLORREF = rgb(132, 150, 156);
pub(super) const fn rgb(red: u32, green: u32, blue: u32) -> COLORREF { red | (green << 8) | (blue << 16) }

pub(super) unsafe fn text(dc: HDC, value: &str, rect: RECT, font: HFONT, color: COLORREF, flags: u32) {
    if rect.right <= rect.left || rect.bottom <= rect.top { return; }
    SelectObject(dc, font);
    SetTextColor(dc, color);
    SetBkMode(dc, TRANSPARENT as i32);
    let mut rect = rect;
    let mut value = wide(value);
    DrawTextW(dc, value.as_mut_ptr(), (value.len() - 1) as i32, &mut rect, flags | DT_NOPREFIX);
}

pub(super) unsafe fn fill(dc: HDC, rect: RECT, brush: HBRUSH) { FillRect(dc, &rect, brush); }

pub(super) unsafe fn wrapped_text(dc: HDC, value: &str, rect: RECT, resources: &Resources, color: COLORREF) {
    for (index, line) in wrapped_lines(dc, value, rect.right - rect.left, resources.font).iter().enumerate() {
        let top = rect.top + index as i32 * resources.typography.advance;
        text(dc, line, RECT { top, bottom: top + resources.typography.cell, ..rect }, resources.font, color, DT_SINGLELINE);
    }
}

pub(super) unsafe fn paint_card(draw: &DRAWITEMSTRUCT, card: &Card, state: &State) {
    let width = draw.rcItem.right - draw.rcItem.left;
    let height = draw.rcItem.bottom - draw.rcItem.top;
    if width <= 0 || height <= 0 { return; }
    let memory = CreateCompatibleDC(draw.hDC);
    let bitmap = CreateCompatibleBitmap(draw.hDC, width, height);
    if !memory.is_null() && !bitmap.is_null() {
        let old = SelectObject(memory, bitmap);
        let buffered = DRAWITEMSTRUCT { hDC: memory,
            rcItem: RECT { left: 0, top: 0, right: width, bottom: height }, ..*draw };
        draw_card(&buffered, card, state);
        BitBlt(draw.hDC, draw.rcItem.left, draw.rcItem.top, width, height, memory, 0, 0, SRCCOPY);
        SelectObject(memory, old);
    } else { draw_card(draw, card, state); }
    if !bitmap.is_null() { DeleteObject(bitmap); }
    if !memory.is_null() { DeleteDC(memory); }
}


pub(super) unsafe fn paint_background(window: HWND, resources: &Resources, marker: Option<RECT>) {
    let mut paint: PAINTSTRUCT = std::mem::zeroed();
    let dc = BeginPaint(window, &mut paint);
    let rect = paint.rcPaint;
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let marker = marker.and_then(|marker| intersection(marker, rect)).map(|mut marker| {
        marker.left -= rect.left;
        marker.top -= rect.top;
        marker.right -= rect.left;
        marker.bottom -= rect.top;
        marker
    });
    if width > 0 && height > 0 && !dc.is_null() {
        let memory = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, width, height);
        if !memory.is_null() && !bitmap.is_null() {
            let old = SelectObject(memory, bitmap);
            fill(memory, RECT { left: 0, top: 0, right: width, bottom: height }, resources.palette.background);
            if let Some(marker) = marker.as_ref() { fill(memory, *marker, resources.palette.teal); }
            BitBlt(dc, rect.left, rect.top, width, height, memory, 0, 0, SRCCOPY);
            SelectObject(memory, old);
        } else {
            fill(dc, rect, resources.palette.background);
            if let Some(marker) = marker.as_ref() {
                fill(dc, RECT { left: marker.left + rect.left, top: marker.top + rect.top,
                    right: marker.right + rect.left, bottom: marker.bottom + rect.top }, resources.palette.teal);
            }
        }
        if !bitmap.is_null() { DeleteObject(bitmap); }
        if !memory.is_null() { DeleteDC(memory); }
    }
    EndPaint(window, &paint);
}

pub(super) unsafe fn draw_item(state: &State, lparam: LPARAM) -> LRESULT {
    let draw = &*(lparam as *const DRAWITEMSTRUCT);
    if draw.CtlType == ODT_STATIC {
        if let Some(card) = state.fixed.iter().chain(&state.cards).find(|card| card.window == draw.hwndItem) {
            paint_card(draw, card, state);
            return 1;
        }
    }
    if draw.CtlType == ODT_BUTTON {
        paint_button(draw, state);
        return 1;
    }
    0
}

pub(super) fn intersection(left: RECT, right: RECT) -> Option<RECT> {
    let rect = RECT { left: left.left.max(right.left), top: left.top.max(right.top),
        right: left.right.min(right.right), bottom: left.bottom.min(right.bottom) };
    (rect.left < rect.right && rect.top < rect.bottom).then_some(rect)
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
    pub(super) teal: HBRUSH,
    pub(super) muted: HBRUSH,
}

impl Palette {
    pub(super) unsafe fn new() -> Self {
        Self { background: CreateSolidBrush(BACKGROUND), teal: CreateSolidBrush(TEAL), muted: CreateSolidBrush(MUTED) }
    }

    pub(super) fn is_valid(&self) -> bool {
        !self.background.is_null() && !self.teal.is_null() && !self.muted.is_null()
    }
}

impl Drop for Palette {
    fn drop(&mut self) {
        unsafe {
            for object in [self.background, self.teal, self.muted] {
                if !object.is_null() { DeleteObject(object); }
            }
        }
    }
}
