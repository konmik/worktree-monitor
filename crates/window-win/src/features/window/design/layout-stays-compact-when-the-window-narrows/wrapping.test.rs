use super::wrap;

#[test]
fn wrapping_preserves_newlines_and_splits_unicode_tokens() {
    let measure = |text: &str| text.chars().count() as i32;
    assert_eq!(wrap("one two\n\n猫犬鳥魚\n", 3, measure), ["one", "two", "", "猫犬鳥", "魚", ""]);
    assert_eq!(wrap("a  bb c", 4, measure), ["a", "bb c"]);
    assert_eq!(wrap("猫犬", 0, measure), ["猫", "犬"]);
}

#[test]
fn wrapping_keeps_graphemes_in_their_display_columns() {
    let measure = |value: &str| crate::render_buffer::display_width(value) as i32;
    assert_eq!(wrap("\u{754c}\u{1f642}e\u{301}\u{1f469}\u{200d}\u{1f4bb}", 3, measure), ["\u{754c}", "\u{1f642}e\u{301}", "\u{1f469}\u{200d}\u{1f4bb}"]);
    assert_eq!(wrap("e\u{301}e\u{301}", 1, measure), ["e\u{301}", "e\u{301}"]);
}
