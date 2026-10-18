use super::optional_count_indicators;
use crate::count_opacity::line_counts_stay_opaque;
use crate::rendering::{GREEN, MUTED, RED};
use crate::render_buffer::Color;
use crate::Counts;
pub(crate) struct CountPart {
    pub(crate) text: String,
    pub(crate) color: Color,
    pub(crate) opaque: bool,
}

pub(crate) fn count_parts(counts: &Counts) -> Vec<CountPart> {
    let changed = line_counts_stay_opaque(counts.source);
    let mut parts = vec![
        CountPart { text: "S ".into(), color: MUTED, opaque: changed },
        CountPart { text: format!("+{}", counts.source.additions), color: if changed { GREEN } else { MUTED }, opaque: changed },
        CountPart { text: "/".into(), color: MUTED, opaque: changed },
        CountPart { text: format!("-{}", counts.source.deletions), color: if changed { RED } else { MUTED }, opaque: changed },
    ];
    optional_count_indicators::append_parts(&mut parts, counts);
    parts
}
