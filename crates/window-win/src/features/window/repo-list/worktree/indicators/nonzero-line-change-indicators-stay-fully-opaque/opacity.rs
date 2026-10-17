pub(crate) fn line_counts_stay_opaque(counts: wtm_core::LineCounts) -> bool {
    counts.additions > 0 || counts.deletions > 0
}
