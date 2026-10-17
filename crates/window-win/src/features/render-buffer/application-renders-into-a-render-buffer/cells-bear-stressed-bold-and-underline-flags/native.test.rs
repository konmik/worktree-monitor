use super::*;
use crate::render_buffer::CellFlags;
use super::super::opaque_indicators::IndicatorBitmap;
use windows_sys::Win32::Graphics::Gdi::{GetDC, ReleaseDC, GdiFlush};

unsafe fn pixels(buffer: &RenderBuffer, resources: &Resources, screen: HDC, layer: TextLayer) -> Vec<u32> {
    let bitmap = IndicatorBitmap::new(screen, buffer.columns() as i32 * resources.space_width,
        (buffer.rows() as i32 - 1) * resources.typography.advance + resources.typography.cell).unwrap();
    present_at(buffer, bitmap.dc, resources, 0, 0, layer);
    GdiFlush();
    std::slice::from_raw_parts(bitmap.pixels, bitmap.length).to_vec()
}

#[test]
fn arbitrary_stressed_cells_use_their_color_and_white_coverage_mask() {
    unsafe {
        let screen = GetDC(null_mut());
        for dpi in [96, 144, 192] {
            let resources = Resources::new(dpi).unwrap();
            let color = Color { red: 240, green: 120, blue: 60 };
            let mut buffer = RenderBuffer::new(9, 2, color);
            buffer.write(0, 0, "normal", color);
            buffer.write_styled(2, 1, "stress", color, CellFlags { stressed: true, ..Default::default() });
            let stressed = pixels(&buffer, &resources, screen, TextLayer::Stressed);
            let mask = pixels(&buffer, &resources, screen, TextLayer::Mask);
            let faded = pixels(&buffer, &resources, screen, TextLayer::Faded);
            let row_length = buffer.columns() * resources.space_width as usize * resources.typography.advance as usize;
            assert!(stressed[..row_length].iter().all(|pixel| *pixel == 0));
            assert!(faded[row_length..].iter().all(|pixel| *pixel == 0));
            assert!(faded[..row_length].iter().any(|pixel| *pixel != 0));
            assert!(mask[row_length..].iter().any(|pixel| *pixel & 0xffffff == 0xffffff));
            for (color, mask) in stressed.iter().zip(&mask) {
                assert_eq!(color & 0xffffff == 0, mask & 0xffffff == 0);
                let red = (color >> 16) & 255;
                let green = (color >> 8) & 255;
                let blue = color & 255;
                assert!(red >= green && green >= blue);
                assert_eq!((mask >> 16) & 255, (mask >> 8) & 255);
                assert_eq!((mask >> 8) & 255, mask & 255);
            }
        }
        ReleaseDC(null_mut(), screen);
    }
}

#[test]
fn bold_and_underline_render_independently_and_together() {
    unsafe {
        let screen = GetDC(null_mut());
        for dpi in [96, 144, 192] {
            let resources = Resources::new(dpi).unwrap();
            let color = Color { red: 255, green: 255, blue: 255 };
            for stressed in [false, true] {
                let layer = if stressed { TextLayer::Stressed } else { TextLayer::All };
                let render = |bold, underline| {
                    let mut buffer = RenderBuffer::new(5, 1, color);
                    buffer.write_styled(0, 0, "Ab c", color, CellFlags { stressed, bold, underline });
                    pixels(&buffer, &resources, screen, layer)
                };
                let plain = render(false, false);
                let bold = render(true, false);
                let underline = render(false, true);
                let combined = render(true, true);
                assert_ne!(plain, bold);
                assert_ne!(plain, underline);
                assert_ne!(bold, combined);
                assert_ne!(underline, combined);
                let in_space = |image: &[u32]| (0..resources.typography.cell).any(|y|
                    (2 * resources.space_width..3 * resources.space_width).any(|x|
                        image[(y * 5 * resources.space_width + x) as usize] != 0));
                assert!(!in_space(&plain));
                assert!(!in_space(&bold));
                assert!(in_space(&underline));
                assert!(in_space(&combined));
            }
        }
        ReleaseDC(null_mut(), screen);
    }
}
