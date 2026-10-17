use crate::git::{category, numstat, worktrees, Category};
use crate::poll::Registrations;
use crate::{Counts, ErrorKind, Head, LineCounts, RepoEntry, Snapshot, WorktreeEntry, WorktreeStats};
use std::path::Path;

include!("../../src/features/change-counting/changes-are-grouped-as-source-tests-tooling-and-binaries/unit.test.rs");

include!("../../src/features/git-execution/git-commands-complete-or-report-failure/process.test.rs");

include!("../../src/features/git-execution/cancellation-stops-git-commands/process.test.rs");

include!("../../src/features/live-monitoring/filesystem-events-refresh-the-affected-targets/signal.test.rs");

include!("../../src/features/snapshot-reporting/snapshots-show-counts-and-errors/unit.test.rs");

include!("../../src/features/repository-discovery/registered-repositories-expose-their-worktrees/unit.test.rs");

include!("../../src/features/live-monitoring/registrations-reload-without-losing-valid-entries/unit.test.rs");
