#[path = "../src/features/repository-registration/users-can-register-and-remove-repositories/registration.rs"]
mod registration;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use wtm_ui::{Cancel, Counts, ErrorKind, Head, LineCounts, Service, WorktreeStats};

const MONITOR_UPDATE_TIMEOUT: Duration = Duration::from_secs(85);

struct Fixture {
    _directory: TempDir,
    repo: PathBuf,
    config: PathBuf,
}

impl Fixture {
    fn new(format: &str) -> Self {
        let parent = std::env::temp_dir().join("opencode");
        fs::create_dir_all(&parent).unwrap();
        let directory = tempfile::Builder::new().prefix("wtm-test-").tempdir_in(parent).unwrap();
        let repo = directory.path().join("repo 树 with spaces");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "--quiet", "--initial-branch=main", &format!("--object-format={format}")]);
        git(&repo, &["config", "core.autocrlf", "false"]);
        let config = directory.path().join("home/.config/wtm/config.json");
        Self { _directory: directory, repo, config }
    }

    fn service(&self) -> Service { Service::at(&self.config) }

    fn register(&self, path: &Path) -> Result<bool, wtm_ui::Error> {
        registration::update(&self.config, path, true, &Cancel::new())
    }

    fn remove(&self, path: &Path) -> Result<bool, wtm_ui::Error> {
        registration::update(&self.config, path, false, &Cancel::new())
    }

    fn write(&self, name: &str, content: impl AsRef<[u8]>) { fs::write(self.repo.join(name), content).unwrap(); }

    fn commit(&self) {
        git(&self.repo, &["add", "."]);
        git(&self.repo, &["-c", "user.name=Wtm Test", "-c", "user.email=wtm@example.invalid", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", "commit", "--quiet", "-m", "Fixture"]);
    }

    fn stats(&self) -> WorktreeStats {
        let snapshot = self.service().snapshot(&Cancel::new());
        assert_eq!(snapshot.config_error, None);
        assert_eq!(snapshot.repos.len(), 1);
        let trees = snapshot.repos[0].worktrees.as_ref().unwrap();
        assert_eq!(trees.len(), 1);
        trees[0].result.clone().unwrap()
    }
}

fn source_counts(additions: u64, deletions: u64, binaries: u64) -> Counts {
    Counts { source: LineCounts { additions, deletions }, binaries, ..Counts::default() }
}

fn git(repo: &Path, args: &[&str]) -> Vec<u8> {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo).args(args);
    for variable in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR"] { command.env_remove(variable); }
    let output = command.output().unwrap();
    assert!(output.status.success(), "Git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    output.stdout
}

fn wait_for(monitor: &wtm_ui::Monitor, condition: impl Fn(&wtm_ui::MonitorUpdate) -> bool) {
    let deadline = Instant::now() + MONITOR_UPDATE_TIMEOUT;
    loop {
        if monitor.latest().is_some_and(|update| condition(&update)) { return; }
        assert!(Instant::now() < deadline, "Timed out waiting for monitor update");
        thread::sleep(Duration::from_millis(30));
    }
}

fn wait_for_within(monitor: &wtm_ui::Monitor,
    condition: impl Fn(&wtm_ui::MonitorUpdate) -> bool, limit: Duration) -> Duration {
    let start = Instant::now();
    let deadline = start + limit;
    loop {
        if monitor.latest().is_some_and(|update| condition(&update)) { return start.elapsed(); }
        assert!(Instant::now() < deadline, "Timed out waiting for monitor update");
        thread::sleep(Duration::from_millis(10));
    }
}

fn settled_polling_time(monitor: &wtm_ui::Monitor) -> Duration {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut previous = monitor.latest().unwrap().polling_time_last_minute;
    let mut unchanged = Instant::now();
    loop {
        let current = monitor.latest().unwrap().polling_time_last_minute;
        if current != previous {
            previous = current;
            unchanged = Instant::now();
        }
        if unchanged.elapsed() >= Duration::from_millis(1200) { return current; }
        assert!(Instant::now() < deadline, "monitor polling work did not settle");
        thread::sleep(Duration::from_millis(30));
    }
}

include!("../../wtm-ui/src/features/change-counting/worktree-counts-include-branch-changes/behavior.test.rs");

include!("../../wtm-ui/src/features/dashboard/repo-list/worktree/indicators/worktrees-show-unpushed-or-unmerged-commit-counts/behavior.test.rs");

include!("../../wtm-ui/src/features/live-monitoring/ignored-directories-do-not-cause-background-scans/behavior.test.rs");

include!("../../wtm-ui/src/features/live-monitoring/scan-passes-are-at-least-ten-seconds-apart/behavior.test.rs");

include!("../src/features/repository-registration/users-can-register-and-remove-repositories/behavior.test.rs");

include!("../../wtm-ui/src/features/repository-discovery/registered-repositories-expose-their-worktrees/behavior.test.rs");

include!("../../wtm-ui/src/features/live-monitoring/launch-directory-repositories-appear-without-registration/behavior.test.rs");

include!("../../wtm-ui/src/features/change-counting/worktree-scans-return-complete-counts-or-an-error/behavior.test.rs");

include!("../../wtm-ui/src/features/git-execution/scans-are-read-only/behavior.test.rs");

include!("../../wtm-ui/src/features/change-counting/changes-are-grouped-as-source-tests-tooling-and-binaries/behavior.test.rs");

include!("../../wtm-ui/src/features/change-counting/non-ignored-untracked-files-contribute-to-counts/behavior.test.rs");

include!("../../wtm-ui/src/features/live-monitoring/registrations-reload-without-losing-valid-entries/behavior.test.rs");

include!("../../wtm-ui/src/features/live-monitoring/filesystem-watchers-recover-after-failures/behavior.test.rs");

include!("../../wtm-ui/src/features/live-monitoring/filesystem-events-refresh-the-affected-targets/behavior.test.rs");
