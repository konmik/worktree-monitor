use super::*;

pub(super) struct CountPart {
    pub(super) text: String,
    pub(super) color: COLORREF,
    pub(super) opaque: bool,
}

pub(super) fn count_parts(counts: &Counts) -> Vec<CountPart> {
    let changed = counts.source.additions > 0 || counts.source.deletions > 0;
    let mut parts = vec![
        CountPart { text: "S ".into(), color: MUTED, opaque: changed },
        CountPart { text: format!("+{}", counts.source.additions), color: if changed { GREEN } else { MUTED }, opaque: changed },
        CountPart { text: "/".into(), color: MUTED, opaque: changed },
        CountPart { text: format!("-{}", counts.source.deletions), color: if changed { RED } else { MUTED }, opaque: changed },
    ];
    for (label, counts) in [("T", counts.test), ("X", counts.extra)] {
        if counts.additions > 0 || counts.deletions > 0 {
            parts.push(CountPart { text: " · ".into(), color: MUTED, opaque: false });
            parts.push(CountPart { text: format!("{label} +{}/-{}", counts.additions, counts.deletions), color: MUTED, opaque: true });
        }
    }
    if counts.binaries > 0 {
        parts.push(CountPart { text: format!(" · binaries: {}", counts.binaries), color: MUTED, opaque: false });
    }
    parts
}

