use super::*;

#[test]
fn renders_source_test_and_extra_counts_in_the_indicator() {
    let snapshot = Snapshot { config_error: None, repos: vec![repo(vec![WorktreeEntry {
        path: "/repo".into(),
        result: Ok(WorktreeStats { pending_commits: 0, head: Head::Branch("main".into()), counts: Counts {
            source: LineCounts { additions: 1, deletions: 2 },
            test: LineCounts { additions: 3, deletions: 4 },
            extra: LineCounts { additions: 5, deletions: 6 },
            binaries: 7,
        } }),
    }])] };
    let view = DashboardView::from_snapshot(&snapshot);
    assert_eq!(view.items[1].display.caption(), "main\nS +1/-2 · T +3/-4 · X +5/-6 · binaries: 7");
}

#[test]
fn shows_test_and_extra_indicators_when_either_counter_is_nonzero() {
    for (test, extra, expected) in [
        (LineCounts { additions: 3, deletions: 0 }, LineCounts::default(), "main\nS +0/-0 · T +3/-0"),
        (LineCounts { additions: 0, deletions: 4 }, LineCounts::default(), "main\nS +0/-0 · T +0/-4"),
        (LineCounts::default(), LineCounts { additions: 5, deletions: 0 }, "main\nS +0/-0 · X +5/-0"),
        (LineCounts::default(), LineCounts { additions: 0, deletions: 6 }, "main\nS +0/-0 · X +0/-6"),
    ] {
        let item = DisplayItem::Worktree { path: "/repo".into(), result: Ok(WorktreeStats { pending_commits: 0,
            head: Head::Branch("main".into()), counts: Counts { test, extra, ..Counts::default() },
        }) };
        assert_eq!(item.caption(), expected);
    }
}

