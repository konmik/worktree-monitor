use super::*;
use super::super::opaque_indicators::IndicatorBitmap;
use windows_sys::Win32::Graphics::Gdi::{GetDC, ReleaseDC, GdiFlush};

#[test]
fn presenter_draws_complete_graphemes_across_their_reserved_columns() {
    unsafe {
        let screen = GetDC(null_mut());
        for dpi in [96, 144, 192] {
            let resources = Resources::new(dpi).unwrap();
            for text in ["\u{754c}", "\u{1f642}", "e\u{301}", "\u{1f469}\u{200d}\u{1f4bb}"] {
                let columns = wtm_dashboard::render_buffer::display_width(text);
                let width = (columns as i32 + 2) * resources.space_width;
                let height = resources.typography.cell + 2;
                let actual = IndicatorBitmap::new(screen, width, height).unwrap();
                let expected = IndicatorBitmap::new(screen, width, height).unwrap();
                let color = Color { red: 255, green: 255, blue: 255 };
                let mut buffer = RenderBuffer::new(columns + 2, 1, color);
                buffer.write(1, 0, text, color);
                present_at(&buffer, actual.dc, &resources, 0, 0, TextLayer::All);
                SelectObject(expected.dc, resources.font);
                SetBkMode(expected.dc, TRANSPARENT as i32);
                SetTextColor(expected.dc, native_color(color));
                let mut rect = RECT { left: resources.space_width, top: 0, right: (columns as i32 + 1) * resources.space_width, bottom: resources.typography.cell };
                let mut value = wide(text);
                DrawTextW(expected.dc, value.as_mut_ptr(), (value.len() - 1) as i32, &mut rect, DT_NOPREFIX | DT_SINGLELINE);
                GdiFlush();
                let actual_pixels = std::slice::from_raw_parts(actual.pixels, actual.length);
                let expected_pixels = std::slice::from_raw_parts(expected.pixels, expected.length);
                assert!(expected_pixels.iter().any(|pixel| *pixel != 0));
                assert_eq!(actual_pixels, expected_pixels, "{text} at {dpi} DPI");
            }
        }
        ReleaseDC(null_mut(), screen);
    }
}
