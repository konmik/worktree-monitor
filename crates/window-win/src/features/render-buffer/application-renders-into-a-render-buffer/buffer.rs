use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CellContent {
    Grapheme(String),
    Continuation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    content: CellContent,
    pub color: Color,
}

impl Cell {
    pub fn text(&self) -> &str {
        match &self.content { CellContent::Grapheme(text) => text, CellContent::Continuation => "" }
    }
    pub fn width(&self) -> usize { display_width(self.text()) }
    fn blank(color: Color) -> Self { Self { content: CellContent::Grapheme(" ".into()), color } }
}

pub fn display_width(value: &str) -> usize {
    value.graphemes(true).map(UnicodeWidthStr::width).sum()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderBuffer {
    columns: usize,
    rows: usize,
    cells: Vec<Cell>,
}

impl RenderBuffer {
    pub fn new(columns: usize, rows: usize, color: Color) -> Self {
        Self { columns, rows, cells: vec![Cell::blank(color); columns.checked_mul(rows).expect("grid dimensions overflow")] }
    }

    pub fn columns(&self) -> usize { self.columns }
    pub fn rows(&self) -> usize { self.rows }
    pub fn cells(&self) -> &[Cell] { &self.cells }
    pub fn row(&self, row: usize) -> Option<&[Cell]> {
        (row < self.rows).then(|| &self.cells[row * self.columns..(row + 1) * self.columns])
    }
    fn clear_glyph(&mut self, column: usize, row: usize) {
        let start = row * self.columns;
        let mut first = column;
        while first > 0 && matches!(self.cells[start + first].content, CellContent::Continuation) { first -= 1; }
        let width = self.cells[start + first].width();
        for x in first..first + width {
            let color = self.cells[start + x].color;
            self.cells[start + x] = Cell::blank(color);
        }
    }
    fn put_grapheme(&mut self, column: usize, row: usize, text: &str, color: Color) {
        let width = display_width(text);
        if row >= self.rows || width == 0 || width > self.columns.saturating_sub(column) { return; }
        for x in column..column + width { self.clear_glyph(x, row); }
        self.cells[row * self.columns + column] = Cell { content: CellContent::Grapheme(text.into()), color };
        for x in column + 1..column + width {
            self.cells[row * self.columns + x] = Cell { content: CellContent::Continuation, color };
        }
    }
    pub fn put(&mut self, column: usize, row: usize, character: char, color: Color) {
        self.write(column, row, character.encode_utf8(&mut [0; 4]), color);
    }
    pub fn write(&mut self, column: usize, row: usize, value: &str, color: Color) {
        if column >= self.columns || row >= self.rows { return; }
        let mut x = column;
        for grapheme in value.graphemes(true) {
            let width = display_width(grapheme);
            if width > self.columns - x { break; }
            self.put_grapheme(x, row, grapheme, color);
            x += width;
        }
    }
    pub fn blit(&mut self, column: usize, row: usize, source: &Self) {
        for y in 0..source.rows.min(self.rows.saturating_sub(row)) {
            for x in 0..source.columns.min(self.columns.saturating_sub(column)) {
                let cell = &source.cells[y * source.columns + x];
                self.put_grapheme(column + x, row + y, cell.text(), cell.color);
            }
        }
    }
}

#[cfg(test)]
#[path = "buffer.test.rs"]
mod tests;
