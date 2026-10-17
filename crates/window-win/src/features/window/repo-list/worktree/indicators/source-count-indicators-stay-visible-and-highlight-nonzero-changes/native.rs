use super::optional_count_indicators;
use super::opaque_indicators::{line_counts_stay_opaque};
use super::window_appearance::{GREEN, MUTED, RED};
use windows_sys::Win32::Foundation::COLORREF;
use wtm_core::Counts;
pub(super) struct CountPart {
    pub(super) text: String,
    pub(super) color: COLORREF,
    pub(super) opaque: bool,
}

pub(super) fn count_parts(counts: &Counts) -> Vec<CountPart> {
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
