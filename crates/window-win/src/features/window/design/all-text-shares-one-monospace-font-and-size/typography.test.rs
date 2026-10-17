use super::*;

#[test]
fn line_origins_follow_the_shared_factor_at_each_dpi() {
    for (dpi, advance, cell) in [(96, 14, 15), (120, 18, 18), (144, 22, 22), (192, 29, 30)] {
        let typography = Typography::new(dpi, cell);
        assert_eq!(typography.top(1) - typography.top(0), advance);
        assert_eq!(typography.top(4) - typography.top(3), advance);
        assert_eq!(typography.height(5) - typography.top(4), cell + 8 * dpi / 96);
    }
}

#[test]
fn wrapping_preserves_newlines_and_splits_unicode_tokens() {
    let measure = |text: &str| text.chars().count() as i32;
    assert_eq!(wrap("one two\n\n猫犬鳥魚\n", 3, measure), ["one", "two", "", "猫犬鳥", "魚", ""]);
    assert_eq!(wrap("a  bb c", 4, measure), ["a", "bb c"]);
    assert_eq!(wrap("猫犬", 0, measure), ["猫", "犬"]);
}
