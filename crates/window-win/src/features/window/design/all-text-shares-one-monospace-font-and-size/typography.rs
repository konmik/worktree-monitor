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


#[cfg(test)]
#[path = "typography.test.rs"]
mod tests;
