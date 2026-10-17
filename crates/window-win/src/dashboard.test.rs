use super::ellipsize_start;

#[test]
fn ellipsize_start_keeps_the_longest_measured_suffix() {
    let measure = |value: &str| value.chars().map(|character| if character == '界' { 2 } else { 1 }).sum();
    assert_eq!(ellipsize_start("ab界cd", 5, measure), "…界cd");
    assert_eq!(ellipsize_start("already short", 20, |value| value.chars().count() as i32), "already short");
}

#[test]
fn ellipsize_start_respects_unicode_boundaries_and_short_widths() {
    let measure = |value: &str| value.chars().count() as i32;
    assert_eq!(ellipsize_start("a🌲cde", 4, measure), "…cde");
    assert_eq!(ellipsize_start("abcdef", 1, measure), "…");
    assert_eq!(ellipsize_start("abcdef", 0, measure), "");
}
