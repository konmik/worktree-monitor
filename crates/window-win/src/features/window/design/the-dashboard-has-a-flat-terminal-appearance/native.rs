use super::*;

pub(super) const BACKGROUND: COLORREF = rgb(11, 16, 18);
pub(super) const GREEN: COLORREF = rgb(194, 249, 112);
pub(super) const TEAL: COLORREF = rgb(84, 213, 197);
pub(super) const RED: COLORREF = rgb(255, 131, 137);
pub(super) const WHITE: COLORREF = rgb(232, 239, 235);
pub(super) const MUTED: COLORREF = rgb(132, 150, 156);
pub(super) const fn rgb(red: u32, green: u32, blue: u32) -> COLORREF { red | (green << 8) | (blue << 16) }

pub(super) unsafe fn text(dc: HDC, value: &str, rect: RECT, font: HFONT, color: COLORREF, flags: u32) {
    if rect.right <= rect.left || rect.bottom <= rect.top { return; }
    SelectObject(dc, font);
    SetTextColor(dc, color);
    SetBkMode(dc, TRANSPARENT as i32);
    let mut rect = rect;
    let mut value = wide(value);
    DrawTextW(dc, value.as_mut_ptr(), (value.len() - 1) as i32, &mut rect, flags | DT_NOPREFIX);
}

pub(super) unsafe fn fill(dc: HDC, rect: RECT, brush: HBRUSH) { FillRect(dc, &rect, brush); }

pub(super) unsafe fn wrapped_text(dc: HDC, value: &str, rect: RECT, resources: &Resources, color: COLORREF) {
    for (index, line) in wrapped_lines(dc, value, rect.right - rect.left, resources.font).iter().enumerate() {
        let top = rect.top + index as i32 * resources.typography.advance;
        text(dc, line, RECT { top, bottom: top + resources.typography.cell, ..rect }, resources.font, color, DT_SINGLELINE);
    }
}

pub(super) unsafe fn paint_card(draw: &DRAWITEMSTRUCT, card: &Card, state: &State) {
    let width = draw.rcItem.right - draw.rcItem.left;
    let height = draw.rcItem.bottom - draw.rcItem.top;
    if width <= 0 || height <= 0 { return; }
    let memory = CreateCompatibleDC(draw.hDC);
    let bitmap = CreateCompatibleBitmap(draw.hDC, width, height);
    if !memory.is_null() && !bitmap.is_null() {
        let old = SelectObject(memory, bitmap);
        let buffered = DRAWITEMSTRUCT { hDC: memory,
            rcItem: RECT { left: 0, top: 0, right: width, bottom: height }, ..*draw };
        draw_card(&buffered, card, state);
        BitBlt(draw.hDC, draw.rcItem.left, draw.rcItem.top, width, height, memory, 0, 0, SRCCOPY);
        SelectObject(memory, old);
    } else { draw_card(draw, card, state); }
    if !bitmap.is_null() { DeleteObject(bitmap); }
    if !memory.is_null() { DeleteDC(memory); }
}

pub(super) unsafe fn draw_card(draw: &DRAWITEMSTRUCT, card: &Card, state: &State) {
    let Some(resources) = &state.resources else { return; };
    let dc = draw.hDC;
    let saved = SaveDC(dc);
    let rect = draw.rcItem;
    let s = |value| scaled(value, state.dpi);
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let line = DT_SINGLELINE | DT_END_ELLIPSIS;
    let caption = card.display.caption();
    let lines: Vec<_> = caption.lines().collect();
    let bounds = |left, top, right, bottom| RECT { left: rect.left + left, top: rect.top + top,
        right: rect.left + right, bottom: rect.top + bottom };
    let typography = resources.typography;
    let line_bounds = |left, index, right| {
        let top = card.spacing.top(typography, index);
        bounds(left, top, right, top + typography.cell)
    };
    let inset = content_inset(width, state.dpi, resources);
    let right_padding = (s(12) - resources.space_width).max(0);
    let heading_right = if matches!(card.display, DisplayItem::Repository { .. }) { width - right_padding }
        else { card.action_rect.map_or(width - right_padding, |rect| rect.left - s(8)) };
    let dragging = state.drag.as_ref().is_some_and(|drag| drag.source_handle == card.window);
    fill(dc, rect, resources.background);
    match &card.display {
        DisplayItem::Status { health, .. } => {
            let color = match health { Health::Live => GREEN, Health::Errors => RED, Health::Scanning => MUTED };
            let y = (height - typography.cell) / 2;
            text(dc, &caption, bounds(0, y, width, y + typography.cell), resources.font, color, line);
        }
        DisplayItem::Repository { bare, .. } => {
            draw_name(dc, lines[0], line_bounds(inset, 0, heading_right), resources.font,
                if dragging { GREEN } else { TEAL }, false, card.refreshing);
            if *bare { text(dc, lines[1], line_bounds(inset, 1, width - right_padding), resources.font, MUTED, line); }
        }
        DisplayItem::Worktree { path, result } => {
            let text_width = content_text_width(width, state.dpi, resources);
            let right = inset + text_width;
            let heading_width = (heading_right - inset).max(0).min(text_width);
            let path = path.display().to_string();
            match result {
                Ok(stats) => {
                    draw_name(dc, lines[0], line_bounds(inset, 0, inset + heading_width), resources.font,
                        if dragging { GREEN } else { WHITE }, false, card.refreshing);
                    let y = card.spacing.top(typography, 1);
                    draw_counts(dc, &stats.counts, bounds(inset, y, right, y + typography.cell), resources, dragging,
                        if state.controls_visible { CountLayer::All } else { CountLayer::Faded });
                }
                Err(error) => {
                    draw_name(dc, &path, line_bounds(inset, 0, inset + heading_width), resources.font,
                        if dragging { GREEN } else { MUTED }, true, card.refreshing);
                    text(dc, lines[1], line_bounds(inset, 1, right), resources.font, MUTED, line);
                    wrapped_text(dc, &format!("ERROR ({:?}): {}", error.kind, error.message), line_bounds(inset, 2, right), resources, RED);
                }
            }
        }
        DisplayItem::PendingWorktree { path } => {
            let heading_width = (heading_right - inset).max(0);
            draw_name(dc, &path.display().to_string(), line_bounds(inset, 0, inset + heading_width), resources.font,
                if dragging { GREEN } else { MUTED }, true, card.refreshing);
        }
        DisplayItem::Notice { title, message, error } => {
            text(dc, title, line_bounds(inset, 0, width - inset), resources.font, if *error { RED } else { WHITE }, line);
            wrapped_text(dc, message, line_bounds(inset, 1, width - inset), resources, if *error { RED } else { MUTED });
        }
    }
    if let Some(target) = state.drag.as_ref().and_then(|drag| drag.target.as_ref()) {
        let y = if target.marker == card.top { Some(0) }
            else if target.marker == card.top + card.height { Some(height - s(2)) } else { None };
        if let Some(y) = y { fill(dc, bounds(0, y, width, y + s(2)), resources.teal); }
    }
    RestoreDC(dc, saved);
}

pub(super) unsafe fn paint_background(window: HWND, resources: &Resources, marker: Option<RECT>) {
    let mut paint: PAINTSTRUCT = std::mem::zeroed();
    let dc = BeginPaint(window, &mut paint);
    let rect = paint.rcPaint;
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let marker = marker.and_then(|marker| intersection(marker, rect)).map(|mut marker| {
        marker.left -= rect.left;
        marker.top -= rect.top;
        marker.right -= rect.left;
        marker.bottom -= rect.top;
        marker
    });
    if width > 0 && height > 0 && !dc.is_null() {
        let memory = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, width, height);
        if !memory.is_null() && !bitmap.is_null() {
            let old = SelectObject(memory, bitmap);
            fill(memory, RECT { left: 0, top: 0, right: width, bottom: height }, resources.background);
            if let Some(marker) = marker.as_ref() { fill(memory, *marker, resources.teal); }
            BitBlt(dc, rect.left, rect.top, width, height, memory, 0, 0, SRCCOPY);
            SelectObject(memory, old);
        } else {
            fill(dc, rect, resources.background);
            if let Some(marker) = marker.as_ref() {
                fill(dc, RECT { left: marker.left + rect.left, top: marker.top + rect.top,
                    right: marker.right + rect.left, bottom: marker.bottom + rect.top }, resources.teal);
            }
        }
        if !bitmap.is_null() { DeleteObject(bitmap); }
        if !memory.is_null() { DeleteDC(memory); }
    }
    EndPaint(window, &paint);
}

