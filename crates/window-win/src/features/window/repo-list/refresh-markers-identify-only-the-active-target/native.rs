use super::*;

pub(super) unsafe fn draw_name(dc: HDC, value: &str, rect: RECT, font: HFONT, color: COLORREF, suffix: bool, refreshing: bool) {
    let marker_width = wrapped_width(dc, "*", font);
    let space_width = wrapped_width(dc, " ", font);
    let available = (rect.right - rect.left - if refreshing { marker_width + space_width } else { 0 }).max(0);
    let measure = |value: &str| wrapped_width(dc, value, font);
    let fitted = if suffix { ellipsize_start(value, available, measure) }
        else if measure(value) <= available { value.to_owned() }
        else if measure("…") > available { String::new() }
        else {
            let mut fitted = "…".to_owned();
            for (index, character) in value.char_indices() {
                let candidate = format!("{}…", &value[..index + character.len_utf8()]);
                if measure(&candidate) > available { break; }
                fitted = candidate;
            }
            fitted
        };
    let name_right = rect.left + measure(&fitted);
    text(dc, &fitted, RECT { right: name_right, ..rect }, font, color, DT_SINGLELINE);
    if refreshing && !fitted.is_empty() {
        text(dc, "*", RECT { left: name_right + space_width, right: name_right + space_width + marker_width, ..rect }, font, TEAL, DT_SINGLELINE);
    }
}

pub(super) unsafe fn refresh_indicators(state: &mut State) {
    if let Some(update) = &state.latest {
        state.refreshing = state.refresh_tracker.update(&update.scan_activities, Instant::now());
    }
    for card in &mut state.cards {
        let refreshing = card.key.as_ref().is_some_and(|key| state.refreshing.contains(key));
        if card.refreshing != refreshing {
            card.refreshing = refreshing;
            SetWindowTextW(card.window, wide(&card.display.caption_with_refresh(refreshing)).as_ptr());
            InvalidateRect(card.window, null(), 0);
        }
    }
}

