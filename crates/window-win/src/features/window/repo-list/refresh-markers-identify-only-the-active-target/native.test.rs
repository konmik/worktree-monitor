use super::buffer_presentation::present_at;
use super::font_resources::Resources;
use super::opaque_indicators::IndicatorBitmap;
use crate::rendering::{render_name, WHITE};
use std::ptr::null_mut;
use windows_sys::Win32::Graphics::Gdi::{GdiFlush, GetDC, IntersectClipRect, ReleaseDC};

#[test]
fn grid_marker_pixels_follow_one_blank_cell_at_each_dpi() {
    unsafe {
        let screen = GetDC(null_mut());
        for dpi in [96, 120, 144, 192] {
            let resources = Resources::new(dpi).unwrap();
            let cell = resources.space_width;
            let left = 7;
            let top = 5;
            let width = left + 12 * cell;
            let height = top + resources.typography.cell + 3;
            let bitmap = IndicatorBitmap::new(screen, width, height).unwrap();
            let buffer = render_name("repository", 7, false, true, WHITE);
            present_at(&buffer, bitmap.dc, &resources, left, top, resources.overlay_font);
            GdiFlush();
            let pixels = std::slice::from_raw_parts(bitmap.pixels, bitmap.length);
            let has_pixels = |start, end| (top..height).any(|y| (start..end).any(|x| pixels[(y * width + x) as usize] != 0));
            assert!(has_pixels(left, left + 5 * cell));
            assert!(!has_pixels(left + 5 * cell, left + 6 * cell));
            assert!(has_pixels(left + 6 * cell, left + 7 * cell));
            assert!(!has_pixels(left + 7 * cell, width));
            let clipped = IndicatorBitmap::new(screen, width, height).unwrap();
            IntersectClipRect(clipped.dc, left + 6 * cell, top, left + 7 * cell, height);
            present_at(&buffer, clipped.dc, &resources, left, top, resources.overlay_font);
            GdiFlush();
            let clipped_pixels = std::slice::from_raw_parts(clipped.pixels, clipped.length);
            for y in 0..height { for x in 0..width {
                if x < left + 6 * cell || x >= left + 7 * cell { assert_eq!(clipped_pixels[(y * width + x) as usize], 0); }
            } }
        }
        ReleaseDC(null_mut(), screen);
    }
}
