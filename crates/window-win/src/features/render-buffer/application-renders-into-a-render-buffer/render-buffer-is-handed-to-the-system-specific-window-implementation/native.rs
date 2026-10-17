use super::font_resources::Resources;
use super::runtime::wide;
use crate::render_buffer::{Color, RenderBuffer};
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{COLORREF, RECT};
use windows_sys::Win32::Graphics::Gdi::{BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DT_NOPREFIX, DT_SINGLELINE, DeleteDC, DeleteObject, DrawTextW, FillRect, HDC, HFONT, RestoreDC, SRCCOPY, SaveDC, SelectObject, SetBkMode, SetTextColor, SetViewportOrgEx, TRANSPARENT};

pub(super) fn native_color(color: Color) -> COLORREF {
    u32::from(color.red) | (u32::from(color.green) << 8) | (u32::from(color.blue) << 16)
}

pub(super) fn columns(width: i32, resources: &Resources) -> usize {
    (width.max(0) / resources.space_width.max(1)) as usize
}

pub(super) unsafe fn present_at(buffer: &RenderBuffer, dc: HDC, resources: &Resources, left: i32, top: i32, font: HFONT) {
    let saved = SaveDC(dc);
    SelectObject(dc, font);
    SetBkMode(dc, TRANSPARENT as i32);
    for row in 0..buffer.rows() {
        for (column, cell) in buffer.row(row).unwrap().iter().enumerate() {
            if cell.text().is_empty() || cell.text() == " " { continue; }
            let x = left + column as i32 * resources.space_width;
            let y = top + row as i32 * resources.typography.advance;
            let mut rect = RECT { left: x, top: y, right: x + cell.width() as i32 * resources.space_width, bottom: y + resources.typography.cell };
            let mut value = wide(cell.text());
            SetTextColor(dc, native_color(cell.color));
            DrawTextW(dc, value.as_mut_ptr(), (value.len() - 1) as i32, &mut rect, DT_NOPREFIX | DT_SINGLELINE);
        }
    }
    RestoreDC(dc, saved);
}

pub(super) unsafe fn paint_at(buffer: &RenderBuffer, dc: HDC, rect: RECT, resources: &Resources, left: i32, top: i32) {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 || dc.is_null() { return; }
    let memory = CreateCompatibleDC(dc);
    let bitmap = CreateCompatibleBitmap(dc, width, height);
    if !memory.is_null() && !bitmap.is_null() {
        let old = SelectObject(memory, bitmap);
        SetViewportOrgEx(memory, -rect.left, -rect.top, null_mut());
        FillRect(memory, &rect, resources.palette.background);
        present_at(buffer, memory, resources, left, top, resources.font);
        SetViewportOrgEx(memory, 0, 0, null_mut());
        BitBlt(dc, rect.left, rect.top, width, height, memory, 0, 0, SRCCOPY);
        SelectObject(memory, old);
    } else {
        FillRect(dc, &rect, resources.palette.background);
        present_at(buffer, dc, resources, left, top, resources.font);
    }
    if !bitmap.is_null() { DeleteObject(bitmap); }
    if !memory.is_null() { DeleteDC(memory); }
}


#[cfg(test)]
#[path = "native.test.rs"]
mod tests;
