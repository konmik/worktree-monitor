use super::count_indicators::{count_parts};
use super::window_appearance::{GREEN, MUTED, RED};
use wtm_core::{Counts};


#[test]
fn line_change_groups_keep_count_text_and_nonzero_opacity() {
    use wtm_core::LineCounts;
    for source in [LineCounts::default(), LineCounts { additions: 3, deletions: 0 }, LineCounts { additions: 0, deletions: 4 }] {
        for test in [LineCounts::default(), LineCounts { additions: 5, deletions: 0 }] {
            for extra in [LineCounts::default(), LineCounts { additions: 0, deletions: 6 }] {
                for binaries in [0, 7] {
                    let counts = Counts { source, test, extra, binaries };
                    let parts = count_parts(&counts);
                    assert_eq!(parts.iter().map(|part| part.text.as_str()).collect::<String>(), counts.to_string());
                    let source_changed = source.additions > 0 || source.deletions > 0;
                    assert!(parts[..4].iter().all(|part| part.opaque == source_changed));
                    assert_eq!(parts[1].color, if source_changed { GREEN } else { MUTED });
                    assert_eq!(parts[3].color, if source_changed { RED } else { MUTED });
                    assert!(parts.iter().filter(|part| part.text.starts_with(" · ")).all(|part| !part.opaque));
                    for label in ["T ", "X "] {
                        assert!(parts.iter().filter(|part| part.text.starts_with(label)).all(|part| part.opaque));
                    }
                }
            }
        }
    }
}

