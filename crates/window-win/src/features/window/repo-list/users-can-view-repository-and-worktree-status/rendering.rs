use crate::dashboard::{DashboardView, DisplayItem, Health};
use crate::render_buffer::{Color, RenderBuffer, display_width};
use crate::name_fitting::fit_name;
use crate::wrapping::wrap;
use wtm_core::Counts;

pub const GREEN: Color = Color { red: 194, green: 249, blue: 112 };
pub const TEAL: Color = Color { red: 84, green: 213, blue: 197 };
pub const RED: Color = Color { red: 255, green: 131, blue: 137 };
pub const WHITE: Color = Color { red: 232, green: 239, blue: 235 };
pub const MUTED: Color = Color { red: 132, green: 150, blue: 156 };

#[derive(Clone, Copy, Default)]
pub enum CountLayer { #[default] All, Faded, Opaque, Mask }

#[derive(Clone, Copy, Default)]
pub struct RowOptions {
    pub refreshing: bool,
    pub dragging: bool,
    pub heading_columns: Option<usize>,
    pub counts: CountLayer,
}

pub fn render_name(value: &str, columns: usize, suffix: bool, refreshing: bool, color: Color) -> RenderBuffer {
    let mut buffer = RenderBuffer::new(columns, 1, MUTED);
    let available = columns.saturating_sub(if refreshing { 2 } else { 0 });
    let name = fit_name(value, available.min(i32::MAX as usize) as i32, suffix, |s| display_width(s) as i32);
    buffer.write(0, 0, &name, color);
    if refreshing && !name.is_empty() { buffer.write(display_width(&name) + 1, 0, "*", TEAL); }
    buffer
}

pub fn render_counts(counts: &Counts, columns: usize, dragging: bool, layer: CountLayer) -> RenderBuffer {
    let mut buffer = RenderBuffer::new(columns, 1, MUTED);
    let parts = crate::count_indicators::count_parts(counts);
    let mut x = 0;
    for part in parts {
        let crate::count_indicators::CountPart { text, color, opaque } = part;
        if match layer { CountLayer::All => true, CountLayer::Faded => !opaque, CountLayer::Opaque | CountLayer::Mask => opaque } {
            buffer.write(x, 0, &text, if matches!(layer, CountLayer::Mask) { Color { red: 255, green: 255, blue: 255 } }
                else if dragging { GREEN } else { color });
        }
        x += display_width(&text);
    }
    buffer
}

pub fn render_row(item: &DisplayItem, columns: usize, options: RowOptions) -> RenderBuffer {
    let caption = item.caption();
    let lines: Vec<_> = caption.lines().collect();
    let heading = options.heading_columns.unwrap_or(columns).min(columns);
    let mut rows = Vec::new();
    let name_color = if options.dragging { GREEN } else { WHITE };
    let mut name = |value: &str, suffix, color| rows.push(render_name(value, heading, suffix, options.refreshing, color));
    match item {
        DisplayItem::Status { health, .. } => {
            let color = match health { Health::Live => GREEN, Health::Errors => RED, Health::Scanning => MUTED };
            let mut row = RenderBuffer::new(columns, 1, MUTED);
            row.write(0, 0, &fit_name(&caption, columns as i32, false, |s| display_width(s) as i32), color);
            rows.push(row);
        }
        DisplayItem::Repository { bare, .. } => {
            name(lines.first().copied().unwrap_or(""), false, if options.dragging { GREEN } else { TEAL });
            if *bare { rows.push(text_row(lines[1], columns, MUTED)); }
        }
        DisplayItem::Worktree { path, result } => match result {
            Ok(stats) => {
                name(lines.first().copied().unwrap_or(""), false, name_color);
                rows.push(render_counts(&stats.counts, columns, options.dragging, options.counts));
            }
            Err(error) => {
                name(&path.display().to_string(), true, if options.dragging { GREEN } else { MUTED });
                rows.push(text_row(lines[1], columns, MUTED));
                for line in wrap(&format!("ERROR ({:?}): {}", error.kind, error.message), columns as i32, |s| display_width(s) as i32) {
                    rows.push(text_row(&line, columns, RED));
                }
            }
        },
        DisplayItem::PendingWorktree { path } => {
            name(&path.display().to_string(), true, if options.dragging { GREEN } else { MUTED });
            rows.resize_with(crate::loading_rows::pending_row_lines(), || RenderBuffer::new(columns, 1, MUTED));
        },
        DisplayItem::Notice { title, message, error } => {
            let color = if *error { RED } else { MUTED };
            rows.push(text_row(title, columns, if *error { RED } else { WHITE }));
            if !message.is_empty() {
                for line in wrap(message, columns as i32, |s| display_width(s) as i32) { rows.push(text_row(&line, columns, color)); }
            }
        }
    }
    let mut buffer = RenderBuffer::new(columns, rows.len(), MUTED);
    for (index, row) in rows.iter().enumerate() { buffer.blit(0, index, row); }
    buffer
}

pub fn text_row(value: &str, columns: usize, color: Color) -> RenderBuffer {
    let mut buffer = RenderBuffer::new(columns, 1, MUTED);
    buffer.write(0, 0, &fit_name(value, columns as i32, false, |s| display_width(s) as i32), color);
    buffer
}

pub fn render_dashboard(view: &DashboardView, polling_time_last_minute: std::time::Duration, columns: usize, rows: usize, options: impl Fn(&crate::dashboard::ViewItem) -> RowOptions) -> RenderBuffer {
    let mut buffer = RenderBuffer::new(columns, rows, MUTED);
    let status = render_row(&DisplayItem::Status { health: view.health, polling_time_last_minute }, columns, RowOptions::default());
    buffer.blit(0, 0, &status);
    let mut y = 2;
    for item in &view.items {
        let row = render_row(&item.display, columns, options(item));
        buffer.blit(0, y, &row);
        y += row.rows() + 1;
        if y >= rows { break; }
    }
    buffer
}

#[cfg(test)]
#[path = "rendering.test.rs"]
mod tests;
