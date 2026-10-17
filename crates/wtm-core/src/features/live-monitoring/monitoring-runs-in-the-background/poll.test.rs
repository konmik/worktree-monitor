use super::*;

fn scan_fixture(count: usize) -> (tempfile::TempDir, Coordinator) {
    let directory = tempfile::tempdir().unwrap();
    let mut repos = Vec::new();
    for index in 0..count {
        let path = directory.path().join(format!("repo-{index}"));
        std::fs::create_dir(&path).unwrap();
        crate::process::checked(Some(&path), &crate::process::args(&["init", "--quiet"]), &Cancel::new()).unwrap();
        repos.push(path.join(".git"));
    }
    let config = directory.path().join("config.json");
    let bytes = serde_json::to_vec(&serde_json::json!({ "repos": repos })).unwrap();
    std::fs::write(&config, &bytes).unwrap();
    let mut coordinator = Coordinator::new(Service::at(config), None, Cancel::new(), Arc::new(Mutex::new(None)));
    coordinator.registrations.reload(Ok(bytes));
    coordinator.refresh_repositories();
    coordinator.launch_changed = false;
    (directory, coordinator)
}

#[path = "../scan-activity-is-published/coordinator.test.rs"]
mod scan_activity_is_published;

#[path = "../discovered-worktrees-join-and-leave-monitoring/coordinator.test.rs"]
mod discovered_worktrees_join_and_leave_monitoring;

#[path = "../scan-passes-are-at-least-ten-seconds-apart/coordinator.test.rs"]
mod scan_passes_are_at_least_ten_seconds_apart;

#[path = "../scan-results-are-published/coordinator.test.rs"]
mod scan_results_are_published;

#[path = "../filesystem-events-refresh-the-affected-targets/coordinator.test.rs"]
mod filesystem_events_refresh_the_affected_targets;

#[path = "../monitoring-work-time-is-reported-for-the-last-minute/coordinator.test.rs"]
mod monitoring_work_time_is_reported_for_the_last_minute;
