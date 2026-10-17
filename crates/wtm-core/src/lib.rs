#[path = "features/change-counting/changes-are-grouped-as-source-tests-tooling-and-binaries/model.rs"]
mod count_model;
#[path = "features/repository-registration/users-can-register-and-remove-repositories/config.rs"]
mod config;
#[path = "features/repository-discovery/registered-repositories-expose-their-worktrees/git.rs"]
mod git;
mod model;
#[path = "features/live-monitoring/monitoring-runs-in-the-background/poll.rs"]
mod poll;
#[path = "features/git-execution/git-commands-complete-or-report-failure/process.rs"]
mod process;
#[path = "features/git-execution/cancellation-stops-git-commands/cancel.rs"]
mod cancellation;
#[path = "features/git-execution/scans-are-read-only/command.rs"]
mod read_only;
#[path = "features/live-monitoring/filesystem-events-refresh-the-affected-targets/watch.rs"]
mod watch;
#[path = "features/live-monitoring/monitoring-runs-in-the-background/service.rs"]
mod monitoring_service;

pub use model::{Counts, Error, ErrorKind, Head, LineCounts, RepoEntry, Snapshot, WorktreeEntry, WorktreeStats};
pub use poll::{Monitor, MonitorUpdate, ScanActivity, ScanTarget};
pub use cancellation::Cancel;
pub use registration_service::Service;

#[path = "features/repository-registration/users-can-register-and-remove-repositories/service.rs"]
mod registration_service;
#[path = "features/snapshot-reporting/snapshots-show-counts-and-errors/service.rs"]
mod snapshot_service;
#[path = "features/snapshot-reporting/snapshots-show-counts-and-errors/report.rs"]
mod snapshot_report;
#[path = "features/snapshot-reporting/snapshots-show-counts-and-errors/counts.rs"]
mod snapshot_counts;

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod unit_tests;
