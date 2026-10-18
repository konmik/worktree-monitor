use super::font_resources::Resources;
use super::runtime::wide;
use wtm_ui::render_buffer::{Color, RenderBuffer};
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{COLORREF, RECT};
use windows_sys::Win32::Graphics::Gdi::{BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DT_NOPREFIX, DT_SINGLELINE, DeleteDC, DeleteObject, DrawTextW, FillRect, HDC, RestoreDC, SRCCOPY, SaveDC, SelectObject, SetBkMode, SetTextColor, SetViewportOrgEx, TRANSPARENT};

#[derive(Clone, Copy)]
pub(super) enum TextLayer { All, Faded, Stressed, Mask }

pub(super) fn native_color(color: Color) -> COLORREF {
    u32::from(color.red) | (u32::from(color.green) << 8) | (u32::from(color.blue) << 16)
}

pub(super) unsafe fn present_at(buffer: &RenderBuffer, dc: HDC, resources: &Resources, left: i32, top: i32, layer: TextLayer) {
    let saved = SaveDC(dc);
    SetBkMode(dc, TRANSPARENT as i32);
    for row in 0..buffer.rows() {
        for (column, cell) in buffer.row(row).unwrap().iter().enumerate() {
            if cell.text().is_empty() || (cell.text() == " " && !cell.flags.underline) { continue; }
            if match layer { TextLayer::All => false, TextLayer::Faded => cell.flags.stressed,
                TextLayer::Stressed | TextLayer::Mask => !cell.flags.stressed } { continue; }
            SelectObject(dc, resources.cell_fonts.font(resources.font, cell.flags, matches!(layer, TextLayer::Stressed | TextLayer::Mask)));
            let x = left + column as i32 * resources.space_width;
            let y = top + row as i32 * resources.typography.advance;
            let mut rect = RECT { left: x, top: y, right: x + cell.width() as i32 * resources.space_width, bottom: y + resources.typography.cell };
            let mut value = wide(cell.text());
            SetTextColor(dc, native_color(if matches!(layer, TextLayer::Mask) { Color { red: 255, green: 255, blue: 255 } } else { cell.color }));
            DrawTextW(dc, value.as_mut_ptr(), (value.len() - 1) as i32, &mut rect, DT_NOPREFIX | DT_SINGLELINE);
        }
    }
    RestoreDC(dc, saved);
}

pub(super) unsafe fn paint_at(buffer: &RenderBuffer, dc: HDC, rect: RECT, resources: &Resources, left: i32, top: i32, layer: TextLayer) {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 || dc.is_null() { return; }
    let memory = CreateCompatibleDC(dc);
    let bitmap = CreateCompatibleBitmap(dc, width, height);
    if !memory.is_null() && !bitmap.is_null() {
        let old = SelectObject(memory, bitmap);
        SetViewportOrgEx(memory, -rect.left, -rect.top, null_mut());
        FillRect(memory, &rect, resources.palette.background);
        present_at(buffer, memory, resources, left, top, layer);
        SetViewportOrgEx(memory, 0, 0, null_mut());
        BitBlt(dc, rect.left, rect.top, width, height, memory, 0, 0, SRCCOPY);
        SelectObject(memory, old);
    } else {
        FillRect(dc, &rect, resources.palette.background);
        present_at(buffer, dc, resources, left, top, layer);
    }
    if !bitmap.is_null() { DeleteObject(bitmap); }
    if !memory.is_null() { DeleteDC(memory); }
}


pub(super) fn compose_frame(state: &super::runtime::State) -> wtm_ui::rendering::DashboardFrame {
    use wtm_ui::rendering::{compose_dashboard, render_row, RowOptions, MUTED, WHITE, GREEN, TEAL};
    use super::visibility_controls::{Control, action_caption};
    let columns = state.columns;
    let rows = state.rows;
    let body_columns = columns.saturating_sub(1);
    let toggle_columns = if state.controls_visible { 12.min(columns) } else { 0 };
    let status = wtm_ui::dashboard::DisplayItem::Status { health: state.view.health,
        polling_time_last_minute: state.latest.as_ref().map_or(std::time::Duration::ZERO, |update| update.polling_time_last_minute) };
    let header = render_row(&status, columns - toggle_columns, RowOptions::default());
    let mut full_header = RenderBuffer::new(columns, 1, MUTED);
    full_header.blit(0, 0, &header);
    if toggle_columns > 0 {
        let control = Control::ShowAll;
        let color = if state.pressed.as_ref() == Some(&control) || state.show_all { GREEN }
            else if state.focused.as_ref() == Some(&control) { TEAL } else { WHITE };
        full_header.blit(columns - toggle_columns, 0, &wtm_ui::rendering::text_row(
            if state.show_all { "[x] Show all" } else { "[ ] Show all" }, toggle_columns, color));
    }
    let items: Vec<_> = state.cards.iter().map(|card| {
        let action = card.action_hidden.filter(|_| state.controls_visible);
        let caption = action.map(|_| action_caption(card));
        let action_columns = caption.map_or(0, |text| wtm_ui::render_buffer::display_width(text) + 2).min(body_columns);
        let mut item = render_row(&card.display, body_columns, RowOptions {
            refreshing: card.refreshing,
            dragging: state.ordering.drag.as_ref().is_some_and(|drag| card.key.as_ref() == Some(&drag.source)),
            heading_columns: Some(body_columns.saturating_sub(action_columns)),
        });
        if let (Some(key), Some(caption)) = (&card.key, caption) {
            let control = Control::Worktree(key.clone());
            let color = if matches!(card.display, wtm_ui::dashboard::DisplayItem::PendingWorktree { .. }) { MUTED }
                else if state.pressed.as_ref() == Some(&control) { GREEN }
                else if state.focused.as_ref() == Some(&control) { TEAL } else { WHITE };
            item.write(body_columns - action_columns, 0, &format!(" {caption} "), color);
        }
        item
    }).collect();
    let attached: Vec<_> = state.cards.windows(2).map(|cards| super::row_layout::attached_rows(&cards[0].display, &cards[1].display)).collect();
    let mut frame = compose_dashboard(&full_header, &items, &attached, columns, rows, state.scrolling.position.max(0) as usize);
    let body_rows = rows.saturating_sub(2);
    if columns > 0 && body_rows > 0 && frame.content_rows > body_rows && state.scrolling.idle.is_some() {
        let column = columns - 1;
        frame.buffer.write(column, 2, "^", MUTED);
        if body_rows > 1 { frame.buffer.write(column, rows - 1, "v", MUTED); }
        let scrollbar = wtm_ui::rendering::Scrollbar::new(frame.content_rows, body_rows, frame.scroll_row);
        for row in 1..body_rows.saturating_sub(1) {
            frame.buffer.write(column, 2 + row, if scrollbar.thumb.contains(&row) { "|" } else { ":" }, MUTED);
        }
    }
    if let Some(marker) = state.ordering.drag.as_ref().and_then(|drag| drag.target.as_ref()).map(|target| target.marker) {
        let marker = marker - frame.scroll_row as i32;
        if marker >= 0 && (marker as usize) <= body_rows && body_rows > 0 {
            let row = 2 + (marker as usize).saturating_sub(1);
            let cells = frame.buffer.row(row).unwrap().to_vec();
            for (column, cell) in cells.iter().take(body_columns).enumerate() {
                if cell.text().is_empty() { continue; }
                frame.buffer.write_styled(column, row, cell.text(), if cell.text() == " " { TEAL } else { cell.color },
                    wtm_ui::render_buffer::CellFlags { underline: true, ..cell.flags });
            }
        }
    }
    frame
}

pub(super) unsafe fn paint_frame(dc: HDC, rect: RECT, state: &super::runtime::State) {
    let Some(resources) = &state.resources else { return; };
    let Some(frame) = &state.frame else { return; };
    paint_at(&frame.buffer, dc, rect, resources, 0, 0,
        if state.controls_visible { TextLayer::All } else { TextLayer::Faded });
}


#[cfg(test)]
#[path = "presentation.test.rs"]
mod tests;

#[cfg(test)]
#[path = "test-support.rs"]
pub(super) mod test_support;

#[cfg(test)]
#[path = "dashboard.test.rs"]
mod dashboard_tests;
