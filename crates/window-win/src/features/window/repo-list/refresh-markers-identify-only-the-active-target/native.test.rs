use super::*;

#[test]
fn refresh_marker_pixels_follow_displayed_name_by_one_space() {
    unsafe {
        let screen = GetDC(null_mut());
        for dpi in [96, 120, 144, 192] {
            let resources = Resources::new(dpi).unwrap();
            for (name, suffix, displayed, narrow) in [
                ("repository", false, "repository", false),
                ("master", false, "master", false),
                ("repository-name", false, "repo…", true),
                ("C:\\work\\branch", true, "…branch", true),
            ] {
                let actual = CreateCompatibleDC(screen);
                let expected = CreateCompatibleDC(screen);
                SelectObject(expected, resources.font);
                let mut name_size = std::mem::zeroed();
                let name_wide = wide(displayed);
                assert_ne!(GetTextExtentPoint32W(expected, name_wide.as_ptr(), (name_wide.len() - 1) as i32, &mut name_size), 0);
                let mut marker_size = std::mem::zeroed();
                assert_ne!(GetTextExtentPoint32W(expected, wide("*").as_ptr(), 1, &mut marker_size), 0);
                let mut space_size = std::mem::zeroed();
                assert_ne!(GetTextExtentPoint32W(expected, wide(" ").as_ptr(), 1, &mut space_size), 0);
                let width = if narrow { name_size.cx + space_size.cx + marker_size.cx } else { scaled(500, dpi) };
                let height = resources.typography.cell;
                let actual_bitmap = CreateCompatibleBitmap(screen, width, height);
                let expected_bitmap = CreateCompatibleBitmap(screen, width, height);
                let actual_old = SelectObject(actual, actual_bitmap);
                let expected_old = SelectObject(expected, expected_bitmap);
                let rect = RECT { left: 0, top: 0, right: width, bottom: height };
                fill(actual, rect, resources.background);
                fill(expected, rect, resources.background);
                draw_name(actual, name, rect, resources.font, WHITE, suffix, true);
                text(expected, displayed, RECT { right: name_size.cx, ..rect }, resources.font, WHITE, DT_SINGLELINE);
                text(expected, "*", RECT { left: name_size.cx + space_size.cx, right: name_size.cx + space_size.cx + marker_size.cx, ..rect }, resources.font, TEAL, DT_SINGLELINE);
                let mut marker_pixels = 0;
                for y in 0..height {
                    for x in 0..width {
                        let pixel = GetPixel(actual, x, y);
                        assert_eq!(pixel, GetPixel(expected, x, y), "DPI {dpi}, {name}, pixel {x},{y}");
                        if x >= name_size.cx + space_size.cx && pixel != BACKGROUND { marker_pixels += 1; }
                    }
                    for x in name_size.cx..name_size.cx + space_size.cx {
                        assert_eq!(GetPixel(actual, x, y), BACKGROUND);
                    }
                }
                assert!(marker_pixels > 0);
                SelectObject(actual, actual_old);
                SelectObject(expected, expected_old);
                DeleteObject(actual_bitmap);
                DeleteObject(expected_bitmap);
                DeleteDC(actual);
                DeleteDC(expected);
            }
        }
        ReleaseDC(null_mut(), screen);
    }
}

