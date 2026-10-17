pub(crate) fn line_counts_stay_opaque(counts: wtm_core::LineCounts) -> bool {
    counts.additions > 0 || counts.deletions > 0
}

use crate::render_buffer::{CellFlags, RenderBuffer, display_width};

pub fn render_counts(counts: &wtm_core::Counts, columns: usize, dragging: bool) -> RenderBuffer {
    let mut buffer = RenderBuffer::new(columns, 1, crate::rendering::MUTED);
    let mut x = 0;
    for part in crate::count_indicators::count_parts(counts) {
        buffer.write_styled(x, 0, &part.text, if dragging { crate::rendering::GREEN } else { part.color },
            CellFlags { stressed: part.opaque, ..Default::default() });
        x += display_width(&part.text);
    }
    buffer
}
