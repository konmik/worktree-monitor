use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn ellipsize_start<F>(value: &str, width: i32, measure: F) -> String
where F: Fn(&str) -> i32 {
    if width <= 0 { return String::new(); }
    if measure(value) <= width { return value.into(); }
    let ellipsis = "…";
    if measure(ellipsis) > width { return String::new(); }
    let mut result = ellipsis.to_owned();
    for (index, _) in value.grapheme_indices(true).rev() {
        let mut candidate = String::with_capacity(ellipsis.len() + value.len() - index);
        candidate.push_str(ellipsis);
        candidate.push_str(&value[index..]);
        if measure(&candidate) <= width { result = candidate; }
    }
    result
}

pub(crate) fn fit_name(value: &str, available: i32, suffix: bool, measure: impl Fn(&str) -> i32) -> String {
    if suffix { ellipsize_start(value, available, measure) }
        else if measure(value) <= available { value.to_owned() }
        else if measure("…") > available { String::new() }
        else {
            let mut fitted = "…".to_owned();
            for (index, character) in value.grapheme_indices(true) {
                let candidate = format!("{}…", &value[..index + character.len()]);
                if measure(&candidate) > available { break; }
                fitted = candidate;
            }
            fitted
        }
}

#[cfg(test)]
#[path = "name-fitting.test.rs"]
mod tests;
