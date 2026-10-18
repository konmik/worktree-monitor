use super::super::buffer_presentation::{TextLayer, test_support::pixels};
use super::super::font_resources::Resources;
use wtm_ui::render_buffer::{CellFlags, Color, RenderBuffer};
use std::ptr::null_mut;
use windows_sys::Win32::Graphics::Gdi::{GetDC, ReleaseDC};

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

