use super::*;
use std::collections::HashSet;
use std::time::Instant;
use wtm_ui::dashboard::RefreshTracker;
use wtm_ui::ScanActivity;

fn scan(path: &str, generation: u64, started_at: Instant, finished_at: Option<Instant>) -> ScanActivity {
    ScanActivity { target: ScanTarget::Worktree { repository: "/repo/.git".into(), path: path.into() },
        generation, started_at, finished_at }
}

#[test]
fn refresh_completed_before_first_observation_lasts_one_second_and_expires_without_snapshot_change() {
    let start = Instant::now();
    let observed = start + Duration::from_secs(10);
    let activities = [scan("/repo", 1, start, Some(start + Duration::from_millis(1)))];
    let mut tracker = RefreshTracker::default();
    assert_eq!(tracker.update(&activities, observed), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, observed + Duration::from_millis(999)), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, observed + Duration::from_secs(1)), HashSet::new());
    assert_eq!(tracker.update(&activities, observed + Duration::from_secs(20)), HashSet::new());
}

#[test]
fn refresh_long_running_activity_stays_visible_until_finished_without_restarting() {
    let start = Instant::now();
    let mut activities = [scan("/repo", 1, start, None)];
    let mut tracker = RefreshTracker::default();
    assert_eq!(tracker.update(&activities, start), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(5)), HashSet::from([refreshing_tree("/repo")]));
    activities[0].finished_at = Some(start + Duration::from_secs(6));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(6)), HashSet::new());
}

#[test]
fn refresh_completion_within_minimum_visibility_does_not_restart_deadline() {
    let start = Instant::now();
    let mut activities = [scan("/repo", 1, start, None)];
    let mut tracker = RefreshTracker::default();
    assert_eq!(tracker.update(&activities, start), HashSet::from([refreshing_tree("/repo")]));
    activities[0].finished_at = Some(start + Duration::from_millis(500));
    assert_eq!(tracker.update(&activities, start + Duration::from_millis(500)), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(1)), HashSet::new());
}

#[test]
fn refresh_sibling_generations_have_independent_deadlines() {
    let start = Instant::now();
    let mut activities = [scan("/repo", 1, start, Some(start)), scan("/sibling", 2, start, Some(start))];
    let mut tracker = RefreshTracker::default();
    assert_eq!(tracker.update(&activities, start), HashSet::from([refreshing_tree("/repo"), refreshing_tree("/sibling")]));
    activities[1].generation = 3;
    assert_eq!(tracker.update(&activities, start + Duration::from_millis(500)), HashSet::from([refreshing_tree("/repo"), refreshing_tree("/sibling")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(1)), HashSet::from([refreshing_tree("/sibling")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_millis(1500)), HashSet::new());
    activities[0].generation = 4;
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(2)), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(3)), HashSet::new());
}

#[test]
fn repository_and_worktree_refreshes_have_independent_visibility() {
    let start = Instant::now();
    let repository = ItemKey::Repository(identity(&PathBuf::from("/repo/.git")));
    let mut activities = [
        ScanActivity { target: ScanTarget::Repository { common_dir: "/repo/.git".into(), path: "/display".into() },
            generation: 1, started_at: start, finished_at: Some(start) },
        scan("/repo", 2, start, Some(start)),
        scan("/sibling", 3, start, Some(start)),
    ];
    let mut tracker = RefreshTracker::default();
    assert_eq!(tracker.update(&activities, start), HashSet::from([repository.clone(), refreshing_tree("/repo"), refreshing_tree("/sibling")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(1)), HashSet::new());
    activities[1].generation = 4;
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(2)), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_millis(2999)), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(3)), HashSet::new());
    activities[0].generation = 5;
    activities[0].finished_at = None;
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(4)), HashSet::from([repository.clone()]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(6)), HashSet::from([repository]));
    activities[0].finished_at = Some(start + Duration::from_secs(7));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(7)), HashSet::new());
}

#[test]
fn refresh_removed_activity_is_pruned_and_reappearance_is_observed_again() {
    let start = Instant::now();
    let mut activities = [scan("/repo", 1, start, Some(start))];
    let mut tracker = RefreshTracker::default();
    assert_eq!(tracker.update(&activities, start), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&[], start + Duration::from_millis(100)), HashSet::new());
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(2)), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(3)), HashSet::new());
    assert_eq!(tracker.update(&[], start + Duration::from_secs(4)), HashSet::new());
    activities[0].generation = 2;
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(5)), HashSet::from([refreshing_tree("/repo")]));
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(6)), HashSet::new());
}

#[cfg(windows)]
#[test]
fn refresh_path_aliases_share_identity_and_do_not_restart_deadline() {
    let start = Instant::now();
    let mut activities = [
        ScanActivity { target: ScanTarget::Repository { common_dir: r"\\?\C:\Repo\.git".into(), path: r"C:\Display".into() },
            generation: 1, started_at: start, finished_at: Some(start) },
        ScanActivity { target: ScanTarget::Worktree { repository: r"\\?\C:\Repo\.git".into(), path: r"\\?\C:\Repo".into() },
            generation: 2, started_at: start, finished_at: Some(start) },
    ];
    let expected = HashSet::from([ItemKey::Repository("c:/repo/.git".into()), ItemKey::Worktree("c:/repo/.git".into(), "c:/repo".into())]);
    let mut tracker = RefreshTracker::default();
    assert_eq!(tracker.update(&activities, start), expected);
    activities[0].target = ScanTarget::Repository { common_dir: "c:/repo/.git".into(), path: "c:/other-display".into() };
    activities[1].target = ScanTarget::Worktree { repository: "c:/repo/.git".into(), path: "c:/repo".into() };
    assert_eq!(tracker.update(&activities, start + Duration::from_millis(500)), expected);
    assert_eq!(tracker.update(&activities, start + Duration::from_secs(1)), HashSet::new());
}

