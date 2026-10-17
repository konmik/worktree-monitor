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

#[test]
fn fitting_keeps_grapheme_clusters_and_measures_terminal_columns() {
    let measure = |value: &str| crate::render_buffer::display_width(value) as i32;
    assert_eq!(super::fit_name("e\u{301}\u{1f642}abc", 4, false, measure), "e\u{301}\u{1f642}\u{2026}");
    assert_eq!(ellipsize_start("abc\u{1f469}\u{200d}\u{1f4bb}e\u{301}", 4, measure), "\u{2026}\u{1f469}\u{200d}\u{1f4bb}e\u{301}");
    assert_eq!(super::fit_name("\u{754c}\u{1f642}", 2, false, measure), "\u{2026}");
}
