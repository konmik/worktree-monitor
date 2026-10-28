use crate::dashboard::{DashboardView, DisplayItem, Health};
use crate::render_buffer::{Color, RenderBuffer, display_width};
use crate::name_fitting::fit_name;
use crate::wrapping::wrap;

pub const GREEN: Color = Color { red: 194, green: 249, blue: 112 };
pub const TEAL: Color = Color { red: 84, green: 213, blue: 197 };
pub const RED: Color = Color { red: 255, green: 131, blue: 137 };
pub const WHITE: Color = Color { red: 232, green: 239, blue: 235 };
pub const MUTED: Color = Color { red: 132, green: 150, blue: 156 };

#[derive(Clone, Copy, Default)]
pub struct RowOptions {
    pub refreshing: bool,
    pub dragging: bool,
    pub heading_columns: Option<usize>,
}

pub fn render_name(value: &str, columns: usize, suffix: bool, refreshing: bool, color: Color) -> RenderBuffer {
    let mut buffer = RenderBuffer::new(columns, 1, MUTED);
    let available = columns.saturating_sub(if refreshing { 2 } else { 0 });
    let name = fit_name(value, available.min(i32::MAX as usize) as i32, suffix, |s| display_width(s) as i32);
    buffer.write(0, 0, &name, color);
    if refreshing && !name.is_empty() { buffer.write(display_width(&name) + 1, 0, "*", TEAL); }
    buffer
}

pub use crate::count_opacity::render_counts;

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
                rows.push(crate::commit_indicators::render_heading(&crate::dashboard::head_caption(&stats.head),
                    stats.pending_commits, heading, options.refreshing, options.dragging, name_color));
                rows.push(render_counts(&stats.counts, columns, options.dragging));
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

pub fn render_dashboard(view: &DashboardView, idle: crate::IdlePercentage, columns: usize, rows: usize, options: impl Fn(&crate::dashboard::ViewItem) -> RowOptions) -> RenderBuffer {
    let status = render_row(&DisplayItem::Status { health: view.health, idle }, columns, RowOptions::default());
    let items: Vec<_> = view.items.iter().map(|item| render_row(&item.display, columns, options(item))).collect();
    compose_dashboard(&status, &items, &[], columns, rows, 0).buffer
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DashboardFrame {
    pub buffer: RenderBuffer,
    pub content_rows: usize,
    pub scroll_row: usize,
    pub row_offsets: Vec<usize>,
    pub row_heights: Vec<usize>,
}

pub fn compose_dashboard(header: &RenderBuffer, items: &[RenderBuffer], attached: &[bool], columns: usize, rows: usize, requested_scroll: usize) -> DashboardFrame {
    let row_heights: Vec<_> = items.iter().map(RenderBuffer::rows).collect();
    let (row_offsets, content_rows) = crate::layout::content_rows(&row_heights, attached).expect("Could not lay out character rows");
    let body_rows = rows.saturating_sub(2);
    let scroll_row = requested_scroll.min(content_rows.saturating_sub(body_rows));
    let mut buffer = RenderBuffer::new(columns, rows, MUTED);
    buffer.blit_rows(0, 0, header, 0, 1);
    let bottom = scroll_row + body_rows;
    for (item, &offset) in items.iter().zip(&row_offsets) {
        let first = offset.max(scroll_row);
        let last = (offset + item.rows()).min(bottom);
        if first < last { buffer.blit_rows(0, 2 + first - scroll_row, item, first - offset, last - first); }
    }
    DashboardFrame { buffer, content_rows, scroll_row, row_offsets, row_heights }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scrollbar {
    pub rows: usize,
    pub thumb: std::ops::Range<usize>,
    maximum: usize,
}

impl Scrollbar {
    pub fn new(content: usize, rows: usize, position: usize) -> Self {
        let track = rows.saturating_sub(2);
        let length = if content > 0 { (track * rows / content).max(1).min(track) } else { track };
        let maximum = content.saturating_sub(rows);
        let start = if maximum > 0 { (track - length) * position.min(maximum) / maximum } else { 0 };
        Self { rows, thumb: 1 + start..1 + start + length, maximum }
    }

    pub fn position_at(&self, row: i32, grab: usize) -> usize {
        let travel = self.rows.saturating_sub(2 + self.thumb.len());
        if travel == 0 { return 0; }
        let top = (i64::from(row) - grab as i64 - 1).clamp(0, travel as i64) as usize;
        (top * self.maximum + travel / 2) / travel
    }
}

#[cfg(test)]
#[path = "rendering.test.rs"]
mod tests;
