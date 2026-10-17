use super::{Color, RenderBuffer};

#[test]
fn grid_exposes_unicode_characters_and_colors_by_row() {
    let base = Color { red: 1, green: 2, blue: 3 };
    let accent = Color { red: 100, green: 200, blue: 255 };
    let mut buffer = RenderBuffer::new(5, 2, base);
    buffer.write(1, 0, "a\u{e9}\u{1f642}extra", accent);
    assert_eq!(buffer.row(0).unwrap().iter().map(|cell| cell.text()).collect::<String>(), " a\u{e9}\u{1f642}");
    assert!(buffer.row(0).unwrap()[1..].iter().all(|cell| cell.color == accent));
    assert!(buffer.row(1).unwrap().iter().all(|cell| cell.text() == " " && cell.color == base));
    assert!(buffer.row(2).is_none());
    assert_eq!(buffer.cells().len(), 10);
    buffer.put(5, 1, 'x', accent);
    buffer.write(usize::MAX, 1, "x", accent);
    assert_eq!(buffer.cells().len(), 10);
}

#[test]
fn composition_clips_at_each_edge_and_accepts_empty_grids() {
    let color = Color { red: 0, green: 0, blue: 0 };
    let mut source = RenderBuffer::new(3, 2, color);
    source.write(0, 0, "abc", color);
    source.write(0, 1, "def", color);
    let mut target = RenderBuffer::new(2, 2, color);
    target.blit(1, 1, &source);
    assert_eq!(target.row(1).unwrap()[1].text(), "a");
    target.blit(usize::MAX, usize::MAX, &source);
    let mut empty = RenderBuffer::new(0, 1, color);
    empty.write(0, 0, "abc", color);
    empty.blit(0, 0, &source);
    assert_eq!(empty.row(0), Some([].as_slice()));
}

#[test]
fn graphemes_reserve_terminal_columns_and_keep_combining_marks() {
    let color = Color { red: 1, green: 2, blue: 3 };
    let mut buffer = RenderBuffer::new(7, 1, color);
    buffer.write(0, 0, "\u{754c}\u{1f469}\u{200d}\u{1f4bb}e\u{301}!", color);
    let row = buffer.row(0).unwrap();
    assert_eq!(row.iter().map(|cell| cell.text()).collect::<String>(), "\u{754c}\u{1f469}\u{200d}\u{1f4bb}e\u{301}! ");
    assert_eq!(row.iter().map(|cell| cell.width()).collect::<Vec<_>>(), [2, 0, 2, 0, 1, 1, 1]);
    assert!(row.iter().all(|cell| cell.color == color));
    assert_eq!(super::display_width(&row.iter().map(|cell| cell.text()).collect::<String>()), 7);
}

#[test]
fn clipping_and_overwriting_keep_wide_glyphs_whole() {
    let base = Color { red: 1, green: 2, blue: 3 };
    let accent = Color { red: 4, green: 5, blue: 6 };
    let mut buffer = RenderBuffer::new(4, 1, base);
    buffer.write(0, 0, "\u{754c}\u{1f642}", accent);
    buffer.write(1, 0, "x", base);
    assert_eq!(buffer.row(0).unwrap().iter().map(|cell| cell.text()).collect::<String>(), " x\u{1f642}");
    buffer.write(1, 0, "\u{754c}", accent);
    assert_eq!(buffer.row(0).unwrap().iter().map(|cell| cell.text()).collect::<String>(), " \u{754c} ");
    buffer.write(3, 0, "\u{1f642}", accent);
    assert_eq!(buffer.row(0).unwrap().iter().map(|cell| cell.text()).collect::<String>(), " \u{754c} ");
    let mut source = RenderBuffer::new(3, 1, base);
    source.write(0, 0, "\u{754c}x", accent);
    let mut target = RenderBuffer::new(2, 1, base);
    target.blit(1, 0, &source);
    assert_eq!(target.row(0).unwrap().iter().map(|cell| cell.text()).collect::<String>(), "  ");
    target.blit(0, 0, &source);
    assert_eq!(target.row(0).unwrap().iter().map(|cell| cell.text()).collect::<String>(), "\u{754c}");
    assert!(target.row(0).unwrap().iter().all(|cell| cell.color == accent));
}

#[test]
fn text_width_matches_the_columns_reserved_by_storage() {
    let color = Color { red: 1, green: 2, blue: 3 };
    let text = "\u{644}\u{627}";
    assert_eq!(super::display_width(text), 2);
    let mut buffer = RenderBuffer::new(super::display_width(text), 1, color);
    buffer.write(0, 0, text, color);
    assert_eq!(buffer.row(0).unwrap().iter().map(|cell| cell.text()).collect::<String>(), text);
}
