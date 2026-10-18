use super::super::buffer_presentation::{TextLayer, test_support::pixels};
use super::super::font_resources::Resources;
use wtm_ui::render_buffer::{CellFlags, Color, RenderBuffer};
use std::ptr::null_mut;
use windows_sys::Win32::Graphics::Gdi::{GetDC, ReleaseDC};

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
