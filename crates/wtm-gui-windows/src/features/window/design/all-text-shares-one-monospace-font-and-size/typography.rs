pub(crate) const FONT_SIZE: i32 = 12;
pub(crate) const LINE_HEIGHT_FACTOR: f64 = 1.2;

#[derive(Clone, Copy)]
pub(crate) struct Typography {
    pub advance: i32,
    pub cell: i32,
}

impl Typography {
    pub fn new(dpi: i32, cell: i32) -> Self {
        Self {
            advance: ((FONT_SIZE as f64 * LINE_HEIGHT_FACTOR * dpi as f64 / 96.0).round() as i32).max(cell).max(1),
            cell: cell.max(1),
        }
    }
}

#[cfg(test)]
#[path = "typography.test.rs"]
mod tests;
