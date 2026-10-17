use super::{RenderBuffer, Resources, HDC, TextLayer, present_at};
use super::super::opaque_indicators::IndicatorBitmap;
use windows_sys::Win32::Graphics::Gdi::GdiFlush;

pub(in crate::native) unsafe fn pixels(buffer: &RenderBuffer, resources: &Resources, screen: HDC, layer: TextLayer) -> Vec<u32> {
    let bitmap = IndicatorBitmap::new(screen, buffer.columns() as i32 * resources.space_width,
        (buffer.rows() as i32 - 1) * resources.typography.advance + resources.typography.cell).unwrap();
    present_at(buffer, bitmap.dc, resources, 0, 0, layer);
    GdiFlush();
    std::slice::from_raw_parts(bitmap.pixels, bitmap.length).to_vec()
}

