use super::*;

#[test]
fn line_advance_fits_the_complete_glyph_at_each_dpi() {
    for (dpi, advance, cell) in [(96, 15, 15), (120, 18, 18), (144, 22, 22), (192, 30, 30)] {
        let typography = Typography::new(dpi, cell);
        assert_eq!(typography.advance, advance);
        assert_eq!(typography.cell, cell);
        assert!(typography.advance >= typography.cell);
    }
}

#[test]
fn line_advance_keeps_scaled_spacing_for_short_glyphs() {
    let typography = Typography::new(192, 20);
    assert_eq!(typography.advance, 29);
    assert_eq!(typography.cell, 20);
}
