#[path = "../filesystem-events-refresh-the-affected-targets/targets.rs"]
mod watch_targets;

#[path = "../filesystem-watchers-recover-after-failures/reconcile.rs"]
mod watch_recovery;

#[path = "../launch-directory-repositories-appear-without-registration/launch.rs"]
mod launch;

#[path = "../discovered-worktrees-join-and-leave-monitoring/discovery.rs"]
mod discovery;

#[path = "../registrations-reload-without-losing-valid-entries/repositories.rs"]
mod repositories;

#[path = "../filesystem-events-refresh-the-affected-targets/events.rs"]
mod events;

#[path = "../scan-activity-is-published/activity.rs"]
mod activity;

#[path = "../scan-results-are-published/publication.rs"]
mod publication;

#[path = "../scan-passes-are-at-least-ten-seconds-apart/schedule.rs"]
mod schedule;
use schedule::ScanPasses;
#[path = "../ignored-directories-do-not-cause-background-scans/pending.rs"]
mod pending;
use pending::PendingScan;
#[path = "../git-scans-run-sequentially/passes.rs"]
mod passes;
#[path = "../reading-passes-start-only-above-97-percent-idle/gate.rs"]
mod idle_gate;
#[cfg(test)]
use schedule::PASS_DELAY;

#[path = "../monitoring-work-time-is-reported-for-the-last-minute/time.rs"]
mod polling_time;
use polling_time::PollingTime;
pub(crate) use polling_time::idle_percent;

#[path = "../registrations-reload-without-losing-valid-entries/registrations.rs"]
mod registrations;
pub(crate) use registrations::Registrations;

use crate::{git, watch, Cancel, Error, ErrorKind, Service, Snapshot, WorktreeStats};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub use activity::{ScanActivity, ScanTarget};
pub use publication::MonitorUpdate;

pub struct Monitor {
    cancel: Cancel,
    latest: Arc<Mutex<Option<Arc<MonitorUpdate>>>>,
    polling_time: PollingTime,
    worker: Option<JoinHandle<()>>,
}

impl Monitor {
    pub(crate) fn start(service: Service, launch_directory: Option<PathBuf>) -> Result<Self, Error> {
        let cancel = Cancel::new();
        let latest = Arc::new(Mutex::new(None));
        let coordinator = Coordinator::new(service, launch_directory, cancel.clone(), latest.clone());
        let polling_time = coordinator.polling_time.clone();
        let worker = thread::Builder::new().name("wtm-monitor".into()).spawn(move || {
            coordinator.run();
        }).map_err(|error| Error::io("Cannot start monitor", error))?;
        Ok(Self { cancel, latest, polling_time, worker: Some(worker) })
    }

    pub fn latest(&self) -> Option<Arc<MonitorUpdate>> {
        let update = self.latest.lock().unwrap_or_else(|error| error.into_inner()).clone()?;
        let polling_time_last_minute = self.polling_time_last_minute();
        if update.polling_time_last_minute == polling_time_last_minute { return Some(update); }
        Some(Arc::new(MonitorUpdate { polling_time_last_minute, ..update.as_ref().clone() }))
    }

    pub fn polling_time_last_minute(&self) -> Duration {
        self.polling_time.total(Instant::now())
    }

    pub fn stop(&mut self) -> Result<(), Error> {
        self.cancel.cancel();
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| Error::new(ErrorKind::Io, "Monitor worker failed"))?;
        }
        Ok(())
    }
}

impl Drop for Monitor {
    fn drop(&mut self) { let _ = self.stop(); }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct WorktreeKey {
    pub common: PathBuf,
    pub path: PathBuf,
}

pub(crate) fn normalize(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy().replace('\\', "/").to_lowercase();
        let text = if let Some(unc) = text.strip_prefix("//?/unc/") {
            format!("//{unc}")
        } else {
            text.strip_prefix("//?/").unwrap_or(&text).to_owned()
        };
        PathBuf::from(text)
    }
    #[cfg(not(windows))]
    { path.to_path_buf() }
}

impl WorktreeKey {
    pub(crate) fn new(common: &Path, path: &Path) -> Self {
        Self { common: normalize(common), path: normalize(path) }
    }
}

struct WorktreeState {
    path: PathBuf,
    last: Option<Result<WorktreeStats, Error>>,
    activity: Option<ScanActivity>,
}

struct RepoState {
    common_dir: PathBuf,
    path: PathBuf,
    discovered: Option<Result<git::DiscoveredRepo, Error>>,
    worktrees: Vec<WorktreeKey>,
    sources: git::MonitoringSources,
    activity: Option<ScanActivity>,
}

struct Coordinator {
    service: Service,
    launch_directory: Option<PathBuf>,
    cancel: Cancel,
    latest: Arc<Mutex<Option<Arc<MonitorUpdate>>>>,
    registrations: Registrations,
    launch_common: Option<PathBuf>,
    launch_error: Option<Error>,
    config_changed: bool,
    launch_changed: bool,
    discoveries: HashSet<PathBuf>,
    repo_order: Vec<PathBuf>,
    repos: HashMap<PathBuf, RepoState>,
    worktrees: HashMap<WorktreeKey, WorktreeState>,
    passes: ScanPasses,
    polling_time: PollingTime,
    snapshot: Arc<Snapshot>,
    watch: watch::WatchBoundary,
    watch_dirty: bool,
    monitors_failed: bool,
    scan_generation: u64,
    #[cfg(test)]
    published: Vec<Arc<MonitorUpdate>>,
}

impl Coordinator {
    fn new(service: Service, launch_directory: Option<PathBuf>, cancel: Cancel,
        latest: Arc<Mutex<Option<Arc<MonitorUpdate>>>>) -> Self {
        let watch = watch::WatchBoundary::new(cancel.clone());
        Self {
            service, launch_directory, cancel, latest,
            registrations: Registrations::default(),
            launch_common: None, launch_error: None,
            config_changed: true, launch_changed: true,
            discoveries: HashSet::new(), repo_order: Vec::new(), repos: HashMap::new(),
            worktrees: HashMap::new(), passes: ScanPasses::default(),
            polling_time: PollingTime::default(), snapshot: Arc::new(Snapshot::default()),
            watch, watch_dirty: true, monitors_failed: false,
            scan_generation: 0,
            #[cfg(test)]
            published: Vec::new(),
        }
    }

    fn run(mut self) {
        while !self.cancel.is_cancelled() {
            let generation = self.cancel.generation();
            self.reconcile_watches();
            self.receive_watch();
            self.reload_registrations();
            self.try_scan_pass(Instant::now());
            self.publish();
            let now = Instant::now();
            let mut wait = Duration::from_secs(1);
            if self.pending() {
                if let Some(next) = self.passes.next { wait = wait.min(next.saturating_duration_since(now)); }
            }
            if let Some(retry) = self.watch.next_retry() { wait = wait.min(retry.saturating_duration_since(now)); }
            self.cancel.wait_since(generation, wait);
        }
        self.finish_scan();
        self.publish_update();
    }

    fn pending(&self) -> bool {
        self.launch_changed || !self.discoveries.is_empty() || !self.passes.changed.is_empty()
            || self.watch.has_pending_plans()
    }

}

#[cfg(test)]
#[path = "poll.test.rs"]
mod tests;
