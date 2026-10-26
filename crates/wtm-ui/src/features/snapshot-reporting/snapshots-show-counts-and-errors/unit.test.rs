#[test]
fn snapshot_text_uses_the_categorized_indicator() {
    let snapshot = Snapshot { config_error: None, repos: vec![RepoEntry {
        common_dir: "/repo/.git".into(), path: "/repo".into(), worktrees: Ok(vec![WorktreeEntry {
            path: "/repo".into(), result: Ok(WorktreeStats { pending_commits: 0, head: Head::Branch("main".into()), counts: Counts {
                source: LineCounts { additions: 1, deletions: 2 },
                test: LineCounts { additions: 3, deletions: 4 },
                extra: LineCounts { additions: 5, deletions: 6 }, binaries: 7,
            } }),
        }]),
    }] };
    assert!(snapshot.text().contains("S +1/-2 · T +3/-4 · X +5/-6 · binaries: 7"));
}
