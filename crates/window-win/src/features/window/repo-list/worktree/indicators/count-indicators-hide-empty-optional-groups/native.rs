use super::count_indicators::CountPart;
use super::opaque_indicators::line_counts_stay_opaque;
use super::window_appearance::MUTED;
use wtm_core::Counts;

pub(super) fn append_parts(parts: &mut Vec<CountPart>, counts: &Counts) {
    for (label, counts) in [("T", counts.test), ("X", counts.extra)] {
        if counts.additions > 0 || counts.deletions > 0 {
            parts.push(CountPart { text: " · ".into(), color: MUTED, opaque: false });
            parts.push(CountPart { text: format!("{label} +{}/-{}", counts.additions, counts.deletions), color: MUTED, opaque: line_counts_stay_opaque(counts) });
        }
    }
    if counts.binaries > 0 {
        parts.push(CountPart { text: format!(" · binaries: {}", counts.binaries), color: MUTED, opaque: false });
    }
}
