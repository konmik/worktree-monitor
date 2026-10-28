#[path = "features/change-counting/changes-are-grouped-as-source-tests-tooling-and-binaries/model.rs"]
mod count_model;
#[path = "features/live-monitoring/registrations-reload-without-losing-valid-entries/configuration.rs"]
pub mod configuration;
use configuration as config;
#[path = "features/repository-discovery/registered-repositories-expose-their-worktrees/git.rs"]
mod git;
#[path = "features/snapshot-reporting/snapshots-show-counts-and-errors/model.rs"]
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
pub use poll::{IdlePercentage, Monitor, MonitorUpdate, ScanActivity, ScanTarget};
pub use cancellation::Cancel;
pub use monitoring_service::Service;

#[path = "features/snapshot-reporting/snapshots-show-counts-and-errors/service.rs"]
mod snapshot_service;
#[path = "features/snapshot-reporting/snapshots-show-counts-and-errors/report.rs"]
mod snapshot_report;
#[path = "features/snapshot-reporting/snapshots-show-counts-and-errors/counts.rs"]
mod snapshot_counts;

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod unit_tests;

#[path = "features/dashboard/repo-list/worktree/indicators/loading-reserves-count-space-without-placeholder-messages/spacing.rs"]
mod loading_rows;

#[path = "features/dashboard/repo-list/worktree/indicators/source-count-indicators-stay-visible-and-highlight-nonzero-changes/counts.rs"]
mod count_indicators;

#[path = "features/dashboard/repo-list/worktree/indicators/count-indicators-hide-empty-optional-groups/counts.rs"]
mod optional_count_indicators;

#[path = "features/dashboard/repo-list/worktree/indicators/nonzero-line-change-indicators-stay-fully-opaque/opacity.rs"]
mod count_opacity;

#[path = "features/dashboard/repo-list/worktree/indicators/worktrees-show-unpushed-or-unmerged-commit-counts/commits.rs"]
mod commit_indicators;

#[path = "features/dashboard/repo-list/users-can-view-repository-and-worktree-status/rendering.rs"]
pub mod rendering;

#[path = "features/render-buffer/application-renders-into-a-render-buffer/buffer.rs"]
pub mod render_buffer;

#[path = "features/dashboard/repo-list/users-can-view-repository-and-worktree-status/dashboard.rs"]
pub mod dashboard;

#[path = "features/dashboard/repo-list/row-order-survives-restart/order.rs"]
pub mod order;

#[path = "features/dashboard/repo-list/worktree/hidden-worktrees-stay-hidden-after-restart/visibility.rs"]
pub mod visibility;

#[path = "features/dashboard/design/layout-stays-compact-when-the-window-narrows/layout.rs"]
mod layout;

#[path = "features/dashboard/design/layout-stays-compact-when-the-window-narrows/name-fitting.rs"]
mod name_fitting;

#[path = "features/dashboard/design/layout-stays-compact-when-the-window-narrows/wrapping.rs"]
mod wrapping;

#[path = "features/dashboard/repo-list/users-can-view-repository-and-worktree-status/scan.rs"]
mod scan_projection;

#[path = "features/dashboard/preference-failures-are-visible-without-losing-current-settings/projection.rs"]
mod preference_failures;

#[path = "features/dashboard/repo-list/worktree/hidden-worktrees-stay-hidden-after-restart/projection.rs"]
mod visibility_projection;

#[cfg(test)]
#[path = "features/dashboard/repo-list/worktree/indicators/source-count-indicators-stay-visible-and-highlight-nonzero-changes/counts.test.rs"]
mod count_indicator_tests;
