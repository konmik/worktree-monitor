use super::{Color, RenderBuffer, display_width};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellFlags {
    pub stressed: bool,
    pub bold: bool,
    pub underline: bool,
}

impl RenderBuffer {
    pub fn put_styled(&mut self, column: usize, row: usize, character: char, color: Color, flags: CellFlags) {
        self.write_styled(column, row, character.encode_utf8(&mut [0; 4]), color, flags);
    }

    pub fn write_styled(&mut self, column: usize, row: usize, value: &str, color: Color, flags: CellFlags) {
        if column >= self.columns || row >= self.rows { return; }
        let mut x = column;
        for grapheme in value.graphemes(true) {
            let width = display_width(grapheme);
            if width > self.columns - x { break; }
            self.put_grapheme(x, row, grapheme, color, flags);
            x += width;
        }
    }
}

#[cfg(test)]
#[path = "buffer.test.rs"]
mod tests;
