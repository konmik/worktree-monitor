use super::*;

#[test]
fn passes_wait_ten_seconds_after_completion_without_sliding() {
    let mut passes = ScanPasses::default();
    let now = Instant::now();
    assert!(passes.ready(now));
    passes.finish(now);
    let key = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo"));
    for second in 1..10 {
        passes.changed.insert(key.clone(), PendingScan::Forced);
        assert!(!passes.ready(now + Duration::from_secs(second)));
    }
    assert!(passes.ready(now + PASS_DELAY));
    assert_eq!(passes.changed.len(), 1);
}
