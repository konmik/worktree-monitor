use super::wrap;

#[test]
fn wrapping_preserves_newlines_and_splits_unicode_tokens() {
    let measure = |text: &str| text.chars().count() as i32;
    assert_eq!(wrap("one two\n\n猫犬鳥魚\n", 3, measure), ["one", "two", "", "猫犬鳥", "魚", ""]);
    assert_eq!(wrap("a  bb c", 4, measure), ["a", "bb c"]);
    assert_eq!(wrap("猫犬", 0, measure), ["猫", "犬"]);
}
