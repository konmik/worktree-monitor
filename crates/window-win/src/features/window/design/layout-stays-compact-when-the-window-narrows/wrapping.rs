use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn wrap(value: &str, width: i32, mut measure: impl FnMut(&str) -> i32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in value.split('\n') {
        let mut remaining = paragraph.trim_end_matches('\r');
        if remaining.is_empty() { lines.push(String::new()); }
        while !remaining.is_empty() {
            let mut fit = 0;
            let mut boundary = None;
            for (index, character) in remaining.grapheme_indices(true) {
                let end = index + character.len();
                if measure(&remaining[..end]) > width.max(1) && fit > 0 { break; }
                fit = end;
                if character.chars().all(char::is_whitespace) && index > 0 { boundary = Some(index); }
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
#[path = "wrapping.test.rs"]
mod tests;
