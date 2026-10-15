pub(crate) const FONT_SIZE: i32 = 12;
pub(crate) const LINE_HEIGHT_FACTOR: f64 = 1.2;

#[derive(Clone, Copy)]
pub(crate) struct Typography {
    pub advance: i32,
    pub cell: i32,
    pub padding: i32,
}

impl Typography {
    pub fn new(dpi: i32, cell: i32) -> Self {
        Self {
            advance: (FONT_SIZE as f64 * LINE_HEIGHT_FACTOR * dpi as f64 / 96.0).round() as i32,
            cell: cell.max(1),
            padding: (8 * dpi / 96).max(1),
        }
    }

    pub fn top(self, line: usize) -> i32 { self.padding + line as i32 * self.advance }

    pub fn height(self, lines: usize) -> i32 {
        self.padding * 2 + lines.saturating_sub(1) as i32 * self.advance + self.cell
    }
}

pub(crate) fn wrap(value: &str, width: i32, mut measure: impl FnMut(&str) -> i32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in value.split('\n') {
        let mut remaining = paragraph.trim_end_matches('\r');
        if remaining.is_empty() { lines.push(String::new()); }
        while !remaining.is_empty() {
            let mut fit = 0;
            let mut boundary = None;
            for (index, character) in remaining.char_indices() {
                let end = index + character.len_utf8();
                if measure(&remaining[..end]) > width.max(1) && fit > 0 { break; }
                fit = end;
                if character.is_whitespace() && index > 0 { boundary = Some(index); }
            }
            if fit == remaining.len() {
                lines.push(remaining.to_owned());
                break;
            }
            let end = boundary.unwrap_or(fit);
            lines.push(remaining[..end].trim_end().to_owned());
            remaining = remaining[end..].trim_start_matches(char::is_whitespace);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
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
}
