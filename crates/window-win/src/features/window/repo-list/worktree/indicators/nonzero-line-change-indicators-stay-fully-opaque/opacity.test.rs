use std::ptr::{null_mut};
use super::font_resources::{Resources, wrapped_width};
use super::opaque_indicators::IndicatorBitmap;
use super::buffer_presentation::{present_at, TextLayer};
use crate::rendering::render_counts;
use windows_sys::Win32::Foundation::{RECT};
use windows_sys::Win32::Graphics::Gdi::{GdiFlush, GetDC, ReleaseDC};
use wtm_core::{Counts};


#[test]
fn opaque_counts_leave_zero_source_and_binary_text_transparent() {
    use wtm_core::LineCounts;
    unsafe {
        let screen = GetDC(null_mut());
        for dpi in [96, 144, 192] {
            let resources = Resources::new(dpi).unwrap();
            let counts = Counts { test: LineCounts { additions: 12, deletions: 0 },
                extra: LineCounts { additions: 0, deletions: 8 }, binaries: 9, ..Counts::default() };
            let width = wrapped_width(screen, &counts.to_string(), resources.font);
            let height = resources.typography.cell;
            let bitmap = IndicatorBitmap::new(screen, width, height).unwrap();
            let rect = RECT { left: 0, top: 0, right: width, bottom: height };
            present_at(&render_counts(&counts, (width / resources.space_width) as usize, false), bitmap.dc, &resources, rect.left, rect.top, TextLayer::Mask);
            GdiFlush();
            let source_end = wrapped_width(screen, "S +0/-0 · ", resources.font);
            let test_end = source_end + wrapped_width(screen, "T +12/-0", resources.font);
            let extra_start = test_end + wrapped_width(screen, " · ", resources.font);
            let extra_end = extra_start + wrapped_width(screen, "X +0/-8", resources.font);
            let pixels = std::slice::from_raw_parts(bitmap.pixels, bitmap.length);
            let has_pixels = |start, end| (0..height).any(|y| (start..end).any(|x| pixels[(y * width + x) as usize] != 0));
            assert!(!has_pixels(0, source_end));
            assert!(has_pixels(source_end, test_end));
            assert!(!has_pixels(test_end, extra_start));
            assert!(has_pixels(extra_start, extra_end));
            assert!(!has_pixels(extra_end, width));
            let blank = IndicatorBitmap::new(screen, width, height).unwrap();
            present_at(&render_counts(&Counts { binaries: 9, ..Counts::default() }, (width / resources.space_width) as usize, false), blank.dc, &resources, rect.left, rect.top, TextLayer::Mask);
            GdiFlush();
            assert!(std::slice::from_raw_parts(blank.pixels, blank.length).iter().all(|pixel| *pixel == 0));
        }
        ReleaseDC(null_mut(), screen);
    }
}

