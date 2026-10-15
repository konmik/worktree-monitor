use crate::{config, git, watch, Cancel, Error, ErrorKind, Service, Snapshot, WorktreeStats};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct MonitorUpdate {
    pub snapshot: Arc<Snapshot>,
    pub launch_error: Option<Error>,
    pub polling_time_last_minute: Duration,
}

pub struct Monitor {
    cancel: Cancel,
    latest: Arc<Mutex<Option<Arc<MonitorUpdate>>>>,
    worker: Option<JoinHandle<()>>,
}

impl Monitor {
    pub(crate) fn start(service: Service, launch_directory: Option<PathBuf>) -> Result<Self, Error> {
        let cancel = Cancel::new();
        let latest = Arc::new(Mutex::new(None));
        let worker_cancel = cancel.clone();
        let worker_latest = latest.clone();
        let worker = thread::Builder::new().name("wts-monitor".into()).spawn(move || {
            worker(service, launch_directory, worker_cancel, worker_latest);
        }).map_err(|error| Error::io("Cannot start monitor", error))?;
        Ok(Self { cancel, latest, worker: Some(worker) })
    }

    pub fn latest(&self) -> Option<Arc<MonitorUpdate>> {
        self.latest.lock().unwrap_or_else(|error| error.into_inner()).clone()
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

#[derive(Default)]
pub(crate) struct Registrations {
    pub repos: Vec<PathBuf>,
    pub error: Option<Error>,
    bytes: Option<Result<Vec<u8>, Error>>,
}

impl Registrations {
    pub(crate) fn reload(&mut self, bytes: Result<Vec<u8>, Error>) -> bool {
        if self.bytes.as_ref() == Some(&bytes) { return false; }
        match bytes.as_ref().map_err(Clone::clone).and_then(|bytes| config::parse(bytes)) {
            Ok(repos) => { self.repos = repos; self.error = None; }
            Err(error) => { self.error = Some(error); }
        }
        self.bytes = Some(bytes);
        true
    }
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

#[derive(Default)]
pub(crate) struct WorktreeBackoff {
    previous: Option<Result<WorktreeStats, Error>>,
    seconds: u64,
}

impl WorktreeBackoff {
    pub(crate) fn observe(&mut self, result: &Result<WorktreeStats, Error>) -> Duration {
        self.seconds = if self.previous.as_ref() == Some(result) {
            (self.seconds * 2).min(30)
        } else { 5 };
        self.previous = Some(result.clone());
        Duration::from_secs(self.seconds)
    }
}

#[derive(Default)]
pub(crate) struct PollingTime {
    completed: Vec<(Instant, Instant)>,
}

impl PollingTime {
    pub(crate) fn record(&mut self, start: Instant, end: Instant) {
        self.completed.push((start, end));
    }

    pub(crate) fn total(&mut self, now: Instant, active: &[(Instant, Option<Instant>)]) -> Duration {
        let window = now.checked_sub(Duration::from_secs(60)).unwrap_or(now);
        self.completed.retain(|(_, end)| *end >= window);
        let mut total = Duration::ZERO;
        for (start, end) in &self.completed {
            total = total.saturating_add(overlap(*start, *end, window, now));
        }
        for (start, end) in active {
            total = total.saturating_add(overlap(*start, end.unwrap_or(now), window, now));
        }
        total
    }
}

fn overlap(start: Instant, end: Instant, window: Instant, now: Instant) -> Duration {
    let from = if start > window { start } else { window };
    let to = if end < now { end } else { now };
    to.saturating_duration_since(from)
}

enum DiscoveryState {
    Pending,
    Ready(git::DiscoveredRepo),
    Failed(Error),
}

struct WorktreeState {
    path: PathBuf,
    last: Option<Result<WorktreeStats, Error>>,
    discovery_error: bool,
    backoff: WorktreeBackoff,
    due: Instant,
    in_flight: Option<u64>,
    revision: u64,
    generation: u64,
}

struct RepoState {
    common_dir: PathBuf,
    path: PathBuf,
    generation: u64,
    next_discovery: Instant,
    discovery_job: Option<u64>,
    revision: u64,
    discovery: DiscoveryState,
    worktrees: Vec<WorktreeKey>,
}

#[derive(Default)]
struct JobTiming {
    started: Option<Instant>,
    ended: Option<Instant>,
}

struct Job {
    handle: JoinHandle<()>,
    cancel: Cancel,
    timing: Arc<Mutex<JobTiming>>,
}

const MAX_JOBS: usize = 4;
const MAX_SCAN_JOBS: usize = MAX_JOBS - 1;
const MAX_DISCOVERY_JOBS: usize = 2;
const RECOVERY_INTERVAL: Duration = Duration::from_secs(5);
const SAFETY_RECONCILE: Duration = Duration::from_secs(300);
const DEBOUNCE: Duration = Duration::from_millis(150);
const DEBOUNCE_HARD: Duration = Duration::from_secs(1);

fn mark_job_started(timing: &Arc<Mutex<JobTiming>>) {
    let mut timing = timing.lock().unwrap_or_else(|error| error.into_inner());
    timing.started = Some(Instant::now());
}

fn mark_job_ended(timing: &Arc<Mutex<JobTiming>>) {
    let mut timing = timing.lock().unwrap_or_else(|error| error.into_inner());
    timing.ended = Some(Instant::now());
}

enum CompletionKind {
    Launch { revision: u64, result: Result<PathBuf, Error> },
    Discovery {
        common: PathBuf,
        generation: u64,
        revision: u64,
        result: Result<git::DiscoveredRepo, Error>,
    },
    Scan {
        key: WorktreeKey,
        generation: u64,
        revision: u64,
        result: Result<WorktreeStats, Error>,
    },
}

struct Completion {
    id: u64,
    kind: CompletionKind,
}

#[derive(Default)]
struct PendingInvalidations {
    config: bool,
    launch: bool,
    discoveries: HashSet<PathBuf>,
    worktrees: HashSet<WorktreeKey>,
    full: bool,
    first: Option<Instant>,
    due: Option<Instant>,
}

struct Coordinator {
    service: Service,
    launch_directory: Option<PathBuf>,
    cancel: Cancel,
    latest: Arc<Mutex<Option<Arc<MonitorUpdate>>>>,
    registrations: Registrations,
    launch_common: Option<PathBuf>,
    launch_error: Option<Error>,
    launch_due: Instant,
    launch_job: Option<u64>,
    repo_order: Vec<PathBuf>,
    repos: HashMap<PathBuf, RepoState>,
    worktrees: HashMap<WorktreeKey, WorktreeState>,
    scan_jobs: HashMap<WorktreeKey, u64>,
    discovery_jobs: HashMap<PathBuf, u64>,
    next_generation: u64,
    next_job: u64,
    sender: mpsc::Sender<Completion>,
    receiver: mpsc::Receiver<Completion>,
    jobs: HashMap<u64, Job>,
    polling_time: PollingTime,
    snapshot: Arc<Snapshot>,
    last_update: Option<Arc<MonitorUpdate>>,
    last_published: Option<Instant>,
    next_config_read: Instant,
    next_reconcile: Instant,
    watch: watch::WatchBoundary,
    watch_dirty: bool,
    pending: PendingInvalidations,
    launch_revision: u64,
    dirty: bool,
}

impl Coordinator {
    fn new(service: Service, launch_directory: Option<PathBuf>, cancel: Cancel,
        latest: Arc<Mutex<Option<Arc<MonitorUpdate>>>>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let now = Instant::now();
        let watch = watch::WatchBoundary::new(cancel.clone());
        Self {
            service,
            launch_directory,
            cancel,
            latest,
            registrations: Registrations::default(),
            launch_common: None,
            launch_error: None,
            launch_due: now,
            launch_job: None,
            repo_order: Vec::new(),
            repos: HashMap::new(),
            worktrees: HashMap::new(),
            scan_jobs: HashMap::new(),
            discovery_jobs: HashMap::new(),
            next_generation: 1,
            next_job: 1,
            sender,
            receiver,
            jobs: HashMap::new(),
            polling_time: PollingTime::default(),
            snapshot: Arc::new(Snapshot::default()),
            last_update: None,
            last_published: None,
            next_config_read: now,
            next_reconcile: now + SAFETY_RECONCILE,
            watch,
            watch_dirty: true,
            pending: PendingInvalidations::default(),
            launch_revision: 0,
            dirty: true,
        }
    }

    fn run(mut self) {
        loop {
            let wake_generation = self.cancel.generation();
            if self.cancel.is_cancelled() { break; }
            let now = Instant::now();
            self.reconcile_watches(now);
            self.receive_watch(now);
            self.receive();
            self.receive_watch(now);
            self.apply_pending(now);
            self.reload(now);
            self.reconcile_watches(now);
            self.reconcile(now);
            self.schedule(now);
            self.publish();
            if self.cancel.is_cancelled() { break; }
            self.cancel.wait_since(wake_generation, self.next_wait(Instant::now()));
        }
        self.shutdown();
    }

    fn reload(&mut self, now: Instant) {
        if now < self.next_config_read { return; }
        self.next_config_read = now + self.recovery_or_safety();
        if self.registrations.reload(config::read(&self.service.config_path)) {
            self.launch_due = now;
            self.refresh_repositories(now);
            self.dirty = true;
        }
        if self.registrations.error.is_some() { self.next_config_read = now + RECOVERY_INTERVAL; }
    }

    fn recovery_or_safety(&self) -> Duration {
        if self.watch.degraded() { RECOVERY_INTERVAL } else { SAFETY_RECONCILE }
    }

    fn watch_targets(&self) -> watch::WatchTargets {
        let repositories = self.repo_order.iter().filter_map(|key| {
            self.repos.get(key).map(|repo| repo.common_dir.clone())
        }).collect();
        let mut worktrees = Vec::new();
        let mut seen = HashSet::new();
        for common in &self.repo_order {
            let Some(repo) = self.repos.get(common) else { continue; };
            for key in &repo.worktrees {
                if !seen.insert(key.clone()) { continue; }
                if let Some(state) = self.worktrees.get(key) {
                    worktrees.push((key.clone(), state.path.clone()));
                }
            }
        }
        watch::WatchTargets {
            config: self.service.config_path.clone(),
            launch: self.launch_directory.clone(),
            repositories,
            worktrees,
        }
    }

    fn reconcile_watches(&mut self, now: Instant) {
        let retry = self.watch.next_retry();
        if !self.watch_dirty && !retry.is_some_and(|due| now >= due) { return; }
        let status = self.watch.reconcile(&self.watch_targets(), now);
        self.watch_dirty = false;
        if status.full { self.enqueue_full(now); }
        if status.degraded {
            self.next_config_read = self.next_config_read.min(now + RECOVERY_INTERVAL);
        }
    }

    fn receive_watch(&mut self, now: Instant) {
        let batch = self.watch.drain(now);
        if !batch.is_empty() { self.enqueue_watch(batch, now); }
    }

    fn enqueue_full(&mut self, now: Instant) {
        let batch = watch::WatchBatch { full: true, ..watch::WatchBatch::default() };
        self.enqueue_watch(batch, now);
        self.watch_dirty = false;
    }

    fn enqueue_watch(&mut self, batch: watch::WatchBatch, now: Instant) {
        let mut repositories = batch.repositories;
        let mut discoveries = batch.discoveries;
        let mut worktrees = batch.worktrees;
        let direct_worktrees = worktrees.clone();
        if batch.full {
            repositories.extend(self.repos.keys().cloned());
            discoveries.extend(self.repos.keys().cloned());
            worktrees.extend(self.worktrees.keys().cloned());
            self.pending.full = true;
            self.pending.config = true;
            self.pending.launch = self.launch_directory.is_some();
            self.launch_revision = self.launch_revision.wrapping_add(1);
            self.watch_dirty = true;
        }
        if batch.config { self.pending.config = true; }
        if batch.launch && self.launch_directory.is_some() {
            self.pending.launch = true;
            self.launch_revision = self.launch_revision.wrapping_add(1);
        }
        for common in discoveries {
            let common = normalize(&common);
            self.pending.discoveries.insert(common.clone());
            repositories.insert(common);
        }
        for common in repositories {
            let common = normalize(&common);
            let keys = self.repos.get(&common).map(|repo| repo.worktrees.clone()).unwrap_or_default();
            if let Some(repo) = self.repos.get_mut(&common) { repo.revision = repo.revision.wrapping_add(1); }
            for key in keys {
                worktrees.insert(key);
            }
        }
        for key in worktrees {
            if let Some(state) = self.worktrees.get_mut(&key) {
                if direct_worktrees.contains(&key) { state.discovery_error = false; }
                state.revision = state.revision.wrapping_add(1);
                self.pending.worktrees.insert(key);
            }
        }
        if batch.topology { self.watch_dirty = true; }
        if self.pending.first.is_none() { self.pending.first = Some(now); }
        let hard = self.pending.first.unwrap() + DEBOUNCE_HARD;
        self.pending.due = Some((now + DEBOUNCE).min(hard));
        let due = self.pending.due.unwrap();
        if self.pending.config { self.next_config_read = self.next_config_read.max(due); }
        if self.pending.launch { self.launch_due = self.launch_due.max(due); }
        for common in &self.pending.discoveries {
            if let Some(repo) = self.repos.get_mut(common) { repo.next_discovery = repo.next_discovery.max(due); }
        }
        for key in &self.pending.worktrees {
            if let Some(state) = self.worktrees.get_mut(key) { state.due = state.due.max(due); }
        }
    }

    fn apply_pending(&mut self, now: Instant) {
        let Some(due) = self.pending.due else { return; };
        if now < due { return; }
        let pending = std::mem::take(&mut self.pending);
        if pending.config { self.next_config_read = now; }
        if pending.launch { self.launch_due = now; }
        for common in pending.discoveries {
            if let Some(repo) = self.repos.get_mut(&common) { repo.next_discovery = now; }
        }
        for key in pending.worktrees {
            if let Some(state) = self.worktrees.get_mut(&key) { state.due = now; }
        }
        if pending.full {
            self.next_config_read = now;
            self.launch_due = now;
            for repo in self.repos.values_mut() { repo.next_discovery = now; }
            for state in self.worktrees.values_mut() { state.due = now; }
        }
    }

    fn reconcile(&mut self, now: Instant) {
        if now < self.next_reconcile { return; }
        self.next_reconcile = now + SAFETY_RECONCILE;
        self.next_config_read = now;
        if self.launch_directory.is_some() { self.launch_due = now; }
        for repo in self.repos.values_mut() { repo.next_discovery = now; }
        for state in self.worktrees.values_mut() {
            if !state.discovery_error { state.due = now; }
        }
    }

    fn pending_due(&self, now: Instant, launch: bool) -> Instant {
        if launch && self.pending.launch {
            self.pending.due.unwrap_or(now).max(now)
        } else {
            now
        }
    }

    fn pending_due_for_worktree(&self, key: &WorktreeKey, now: Instant) -> Instant {
        if self.pending.worktrees.contains(key) {
            self.pending.due.unwrap_or(now).max(now)
        } else {
            now
        }
    }

    fn pending_due_for_discovery(&self, common: &Path, now: Instant) -> Instant {
        if self.pending.discoveries.contains(common) {
            self.pending.due.unwrap_or(now).max(now)
        } else {
            now
        }
    }

    fn desired_repositories(&self) -> Vec<(PathBuf, PathBuf)> {
        let mut result = Vec::new();
        let mut seen = HashSet::new();
        for common in &self.registrations.repos {
            let key = normalize(common);
            if seen.insert(key.clone()) { result.push((key, common.clone())); }
        }
        if let Some(common) = &self.launch_common {
            let key = normalize(common);
            if seen.insert(key.clone()) { result.push((key, common.clone())); }
        }
        result
    }

    fn refresh_repositories(&mut self, now: Instant) {
        let desired = self.desired_repositories();
        let desired_keys: HashSet<_> = desired.iter().map(|(key, _)| key.clone()).collect();
        let removed: Vec<_> = self.repos.keys().filter(|key| !desired_keys.contains(*key)).cloned().collect();
        let changed = !removed.is_empty() || desired.len() != self.repos.len()
            || self.repo_order != desired.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>();
        for key in removed {
            self.cancel_jobs_for_common(&key);
            self.repos.remove(&key);
            self.worktrees.retain(|tree, _| tree.common != key);
        }
        self.repo_order = desired.iter().map(|(key, _)| key.clone()).collect();
        for (key, common) in desired {
            if let Some(repo) = self.repos.get_mut(&key) {
                repo.common_dir = common;
            } else {
                let generation = self.fresh_generation();
                self.repos.insert(key, RepoState {
                    path: display_path(&common),
                    common_dir: common,
                    generation,
                    next_discovery: now,
                    discovery_job: None,
                    revision: 0,
                    discovery: DiscoveryState::Pending,
                    worktrees: Vec::new(),
                });
            }
        }
        if changed { self.watch_dirty = true; }
    }

    fn cancel_job(&self, id: Option<u64>) {
        if let Some(id) = id {
            if let Some(job) = self.jobs.get(&id) { job.cancel.cancel(); }
        }
    }

    fn cancel_scan(&self, key: &WorktreeKey) {
        self.cancel_job(self.scan_jobs.get(key).copied());
    }

    fn cancel_jobs_for_common(&self, common: &Path) {
        self.cancel_job(self.discovery_jobs.get(common).copied());
        let ids: Vec<_> = self.scan_jobs.iter()
            .filter(|(key, _)| key.common == common)
            .map(|(_, id)| *id)
            .collect();
        for id in ids { self.cancel_job(Some(id)); }
    }

    fn fresh_generation(&mut self) -> u64 {
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1);
        generation
    }

    fn fresh_job(&mut self) -> u64 {
        let id = self.next_job;
        self.next_job = self.next_job.wrapping_add(1);
        id
    }

    fn active_scan_jobs(&self) -> usize {
        self.scan_jobs.values().filter(|id| self.jobs.contains_key(id)).count()
    }

    fn receive(&mut self) {
        while let Ok(completion) = self.receiver.try_recv() {
            let Some(job) = self.jobs.remove(&completion.id) else { continue; };
            let _ = job.handle.join();
            let timing = job.timing.lock().unwrap_or_else(|error| error.into_inner());
            let started = timing.started;
            let ended = timing.ended.unwrap_or_else(Instant::now);
            drop(timing);
            if let Some(started) = started { self.polling_time.record(started, ended); }
            self.complete(completion, ended);
            self.dirty = true;
        }
    }

    fn complete(&mut self, completion: Completion, now: Instant) {
        match completion.kind {
            CompletionKind::Launch { revision, result } => {
                if self.launch_job != Some(completion.id) { return; }
                self.launch_job = None;
                if revision != self.launch_revision {
                    self.launch_due = self.pending_due(now, true);
                    return;
                }
                self.launch_due = now + self.recovery_or_safety();
                match result {
                    Ok(common) => {
                        self.launch_error = None;
                        if self.launch_common.as_ref().map(|path| normalize(path)) != Some(normalize(&common)) {
                            self.launch_common = Some(common);
                            self.refresh_repositories(now);
                        } else {
                            self.launch_common = Some(common);
                        }
                    }
                    Err(error) if error.kind == ErrorKind::Cancelled && self.cancel.is_cancelled() => {}
                    Err(error) if is_not_git(&error) => { self.launch_error = None; }
                    Err(error) => {
                        self.launch_error = Some(error);
                        self.launch_due = now + RECOVERY_INTERVAL;
                    }
                }
            }
            CompletionKind::Discovery { common, generation, revision, result } => {
                if self.discovery_jobs.get(&common) == Some(&completion.id) {
                    self.discovery_jobs.remove(&common);
                }
                let valid = self.repos.get(&common)
                    .is_some_and(|repo| repo.generation == generation && repo.discovery_job == Some(completion.id));
                if !valid { return; }
                let recovery_or_safety = self.recovery_or_safety();
                let pending_due = self.pending_due_for_discovery(&common, now);
                if let Some(repo) = self.repos.get_mut(&common) {
                    repo.discovery_job = None;
                    repo.next_discovery = now + recovery_or_safety;
                    if revision != repo.revision {
                        repo.next_discovery = pending_due;
                    } else if result.is_err() {
                        repo.next_discovery = now + RECOVERY_INTERVAL;
                    }
                }
                if self.repos.get(&common).is_some_and(|repo| revision != repo.revision) { return; }
                self.apply_discovery(&common, generation, result, now);
            }
            CompletionKind::Scan { key, generation, revision, result } => {
                if self.scan_jobs.get(&key) == Some(&completion.id) { self.scan_jobs.remove(&key); }
                let pending_due = self.pending_due_for_worktree(&key, now);
                let recovery_or_safety = self.recovery_or_safety();
                let Some(state) = self.worktrees.get_mut(&key) else { return; };
                if state.generation != generation || state.in_flight != Some(completion.id) { return; }
                state.in_flight = None;
                if state.discovery_error { return; }
                if state.revision != revision {
                    state.due = pending_due;
                    return;
                }
                let delay = state.backoff.observe(&result);
                let delay = if result.is_ok() { recovery_or_safety } else { delay };
                state.last = Some(result);
                state.due = now + delay;
            }
        }
    }

    fn apply_discovery(&mut self, common: &Path, generation: u64,
        result: Result<git::DiscoveredRepo, Error>, now: Instant) {
        let Some(repo) = self.repos.get(common) else { return; };
        if repo.generation != generation { return; }
        let old_keys = repo.worktrees.clone();
        if let Err(error) = result {
            if let Some(repo) = self.repos.get_mut(common) {
                repo.discovery = DiscoveryState::Failed(error);
            }
            return;
        }
        let mut discovered = result.unwrap();
        let common_path = self.repos[common].common_dir.clone();
        let mut seen = HashSet::new();
        discovered.trees.retain(|tree| seen.insert(WorktreeKey::new(&common_path, &tree.path)));
        let new_keys: Vec<_> = discovered.trees.iter()
            .map(|tree| WorktreeKey::new(&self.repos[common].common_dir, &tree.path)).collect();
        let new_set: HashSet<_> = new_keys.iter().cloned().collect();
        let topology_changed = old_keys != new_keys;
        for key in old_keys {
            if !new_set.contains(&key) {
                self.cancel_scan(&key);
                self.worktrees.remove(&key);
            }
        }
        for tree in &discovered.trees {
            let key = WorktreeKey::new(&common_path, &tree.path);
            self.apply_tree(&key, tree, now);
        }
        if let Some(repo) = self.repos.get_mut(common) {
            repo.path = discovered.path.clone();
            repo.worktrees = new_keys;
            repo.discovery = DiscoveryState::Ready(discovered);
        }
        if topology_changed { self.watch_dirty = true; }
    }

    fn apply_tree(&mut self, key: &WorktreeKey, tree: &git::DiscoveredTree, now: Instant) {
        if let Some(error) = &tree.error {
            let keep = self.worktrees.get(key).is_some_and(|state| state.discovery_error
                && state.last.as_ref() == Some(&Err(error.clone())));
            if keep {
                if let Some(state) = self.worktrees.get_mut(key) { state.path = tree.path.clone(); }
                return;
            }
            self.cancel_scan(key);
            let generation = self.fresh_generation();
            self.worktrees.insert(key.clone(), WorktreeState {
                path: tree.path.clone(),
                last: Some(Err(error.clone())),
                discovery_error: true,
                backoff: WorktreeBackoff::default(),
                due: now + Duration::from_secs(1),
                in_flight: None,
                revision: 0,
                generation,
            });
            return;
        }
        let keep = self.worktrees.get(key).is_some_and(|state| !state.discovery_error);
        if keep {
            if let Some(state) = self.worktrees.get_mut(key) { state.path = tree.path.clone(); }
            return;
        }
        self.cancel_scan(key);
        let generation = self.fresh_generation();
        self.worktrees.insert(key.clone(), WorktreeState {
            path: tree.path.clone(),
            last: None,
            discovery_error: false,
            backoff: WorktreeBackoff::default(),
            due: now,
            in_flight: None,
            revision: 0,
            generation,
        });
    }

    fn schedule(&mut self, now: Instant) {
        if self.launch_directory.is_some() && self.launch_job.is_none() && now >= self.launch_due {
            self.start_launch(now);
        }
        let mut repo_targets: Vec<_> = self.repo_order.iter().filter_map(|key| {
            self.repos.get(key).and_then(|repo| (repo.discovery_job.is_none()
                && !self.discovery_jobs.contains_key(key) && now >= repo.next_discovery)
                .then(|| (key.clone(), repo.common_dir.clone(), repo.generation, repo.revision, repo.next_discovery)))
        }).collect();
        repo_targets.sort_by_key(|(_, _, _, _, due)| *due);
        for (key, common, generation, revision, _) in repo_targets {
            self.start_discovery(key, common, generation, revision, now);
        }

        let mut scan_targets = Vec::new();
        let mut scheduled = HashSet::new();
        for common in &self.repo_order {
            let Some(repo) = self.repos.get(common) else { continue; };
            if !matches!(repo.discovery, DiscoveryState::Ready(_)) { continue; }
            for key in &repo.worktrees {
                if !scheduled.insert(key.clone()) { continue; }
                let Some(state) = self.worktrees.get(key) else { continue; };
                if state.in_flight.is_none() && !self.scan_jobs.contains_key(key)
                    && !state.discovery_error && now >= state.due {
                    scan_targets.push((key.clone(), state.path.clone(), state.generation,
                        state.revision, state.due));
                }
            }
        }
        scan_targets.sort_by_key(|(_, _, _, _, due)| *due);
        for (key, path, generation, revision, _) in scan_targets {
            self.start_scan(key, path, generation, revision, now);
        }
    }

    fn start_launch(&mut self, now: Instant) {
        if self.jobs.len() >= MAX_JOBS { return; }
        let Some(directory) = self.launch_directory.clone() else { return; };
        let id = self.fresh_job();
        let sender = self.sender.clone();
        let cancel = Cancel::new();
        let worker_cancel = cancel.clone();
        let wake = self.cancel.clone();
        let revision = self.launch_revision;
        let timing = Arc::new(Mutex::new(JobTiming::default()));
        let worker_timing = timing.clone();
        let spawn = thread::Builder::new().name("wts-launch-discovery".into()).spawn(move || {
            mark_job_started(&worker_timing);
            let result = config::resolve(&directory, &worker_cancel);
            mark_job_ended(&worker_timing);
            let _ = sender.send(Completion { id, kind: CompletionKind::Launch { revision, result } });
            wake.wake();
        });
        match spawn {
            Ok(handle) => {
                self.jobs.insert(id, Job { handle, cancel, timing });
                self.launch_job = Some(id);
                self.launch_due = now + self.recovery_or_safety();
            }
            Err(error) => {
                self.launch_error = Some(Error::io("Cannot start launch discovery", error));
                self.launch_due = now + RECOVERY_INTERVAL;
            }
        }
    }

    fn start_discovery(&mut self, common_key: PathBuf, common: PathBuf, generation: u64,
        revision: u64, now: Instant) {
        if self.jobs.len() >= MAX_JOBS || self.discovery_jobs.len() >= MAX_DISCOVERY_JOBS { return; }
        let id = self.fresh_job();
        let sender = self.sender.clone();
        let cancel = Cancel::new();
        let worker_cancel = cancel.clone();
        let wake = self.cancel.clone();
        let timing = Arc::new(Mutex::new(JobTiming::default()));
        let worker_timing = timing.clone();
        let thread_common = common.clone();
        let spawn = thread::Builder::new().name("wts-repository-discovery".into()).spawn(move || {
            mark_job_started(&worker_timing);
            let result = git::discover(&thread_common, &worker_cancel);
            mark_job_ended(&worker_timing);
            let _ = sender.send(Completion { id, kind: CompletionKind::Discovery {
                common: common_key, generation, revision, result,
            }});
            wake.wake();
        });
        match spawn {
            Ok(handle) => {
                self.jobs.insert(id, Job { handle, cancel, timing });
                self.discovery_jobs.insert(normalize(&common), id);
                if let Some(repo) = self.repos.get_mut(&normalize(&common)) {
                    repo.discovery_job = Some(id);
                }
            }
            Err(error) => {
                if let Some(repo) = self.repos.get_mut(&normalize(&common)) {
                    repo.discovery = DiscoveryState::Failed(Error::io("Cannot start repository discovery", error));
                    repo.next_discovery = now + RECOVERY_INTERVAL;
                }
            }
        }
    }

    fn start_scan(&mut self, key: WorktreeKey, path: PathBuf, generation: u64,
        revision: u64, now: Instant) {
        if self.jobs.len() >= MAX_JOBS || self.active_scan_jobs() >= MAX_SCAN_JOBS { return; }
        let id = self.fresh_job();
        let sender = self.sender.clone();
        let cancel = Cancel::new();
        let worker_cancel = cancel.clone();
        let wake = self.cancel.clone();
        let timing = Arc::new(Mutex::new(JobTiming::default()));
        let worker_timing = timing.clone();
        let completion_key = key.clone();
        let spawn = thread::Builder::new().name("wts-worktree-scan".into()).spawn(move || {
            mark_job_started(&worker_timing);
            let result = git::scan_tree(&path, &worker_cancel);
            mark_job_ended(&worker_timing);
            let _ = sender.send(Completion { id, kind: CompletionKind::Scan {
                key: completion_key, generation, revision, result,
            } });
            wake.wake();
        });
        match spawn {
            Ok(handle) => {
                self.jobs.insert(id, Job { handle, cancel, timing });
                self.scan_jobs.insert(key.clone(), id);
                if let Some(state) = self.worktrees.get_mut(&key) {
                    state.in_flight = Some(id);
                }
            }
            Err(error) => {
                if let Some(state) = self.worktrees.get_mut(&key) {
                    let result = Err(Error::io("Cannot start worktree scan", error));
                    let delay = state.backoff.observe(&result);
                    state.last = Some(result);
                    state.due = now + delay;
                }
            }
        }
    }

    fn build_snapshot(&mut self) -> Snapshot {
        let mut repos = Vec::new();
        for common in &self.repo_order {
            let Some(repo) = self.repos.get_mut(common) else { continue; };
            let entry = match &repo.discovery {
                DiscoveryState::Pending => None,
                DiscoveryState::Failed(error) => Some(crate::RepoEntry {
                    common_dir: repo.common_dir.clone(),
                    path: repo.path.clone(),
                    worktrees: Err(error.clone()),
                }),
                DiscoveryState::Ready(discovered) => {
                    let complete = discovered.trees.iter().all(|tree| {
                        let key = WorktreeKey::new(&repo.common_dir, &tree.path);
                        self.worktrees.get(&key).and_then(|state| state.last.as_ref()).is_some()
                    });
                    let trees: Vec<_> = discovered.trees.iter().filter_map(|tree| {
                        let key = WorktreeKey::new(&repo.common_dir, &tree.path);
                        self.worktrees.get(&key).and_then(|state| state.last.as_ref().map(|result| {
                            crate::WorktreeEntry { path: state.path.clone(), result: result.clone() }
                        }))
                    }).collect();
                    if !complete && trees.is_empty() { None } else {
                        Some(crate::RepoEntry {
                            common_dir: repo.common_dir.clone(),
                            path: repo.path.clone(),
                            worktrees: Ok(trees),
                        })
                    }
                }
            };
            if let Some(entry) = entry { repos.push(entry); }
        }
        Snapshot { repos, config_error: self.registrations.error.clone() }
    }

    fn publish(&mut self) {
        let snapshot = self.build_snapshot();
        if self.snapshot.as_ref() != &snapshot {
            self.snapshot = Arc::new(snapshot);
            self.dirty = true;
        }
        if self.last_update.is_none() && self.initial_pending(self.snapshot.as_ref()) { return; }
        let now = Instant::now();
        let active: Vec<_> = self.jobs.values().filter_map(|job| {
            let timing = job.timing.lock().unwrap_or_else(|error| error.into_inner());
            timing.started.map(|started| (started, timing.ended))
        }).collect();
        let polling_time_last_minute = self.polling_time.total(now, &active);
        let heartbeat = self.last_published.is_none_or(|last| now.duration_since(last) >= Duration::from_secs(1));
        let changed = self.last_update.as_ref().is_none_or(|update| {
            update.snapshot.as_ref() != self.snapshot.as_ref() || update.launch_error != self.launch_error
        });
        if self.dirty || heartbeat || changed {
            let update = Arc::new(MonitorUpdate {
                snapshot: self.snapshot.clone(),
                launch_error: self.launch_error.clone(),
                polling_time_last_minute,
            });
            *self.latest.lock().unwrap_or_else(|error| error.into_inner()) = Some(update.clone());
            self.last_update = Some(update);
            self.last_published = Some(now);
            self.dirty = false;
        }
    }

    fn initial_pending(&self, snapshot: &Snapshot) -> bool {
        if !snapshot.repos.is_empty() || snapshot.config_error.is_some() || self.launch_error.is_some() { return false; }
        if self.last_update.as_ref().is_some_and(|update| !update.snapshot.repos.is_empty()) { return false; }
        if self.repos.is_empty() { return self.launch_job.is_some() && self.launch_common.is_none(); }
        self.repos.values().any(|repo| match &repo.discovery {
            DiscoveryState::Pending => true,
            DiscoveryState::Failed(_) => false,
            DiscoveryState::Ready(discovered) => discovered.trees.iter().any(|tree| {
                let key = WorktreeKey::new(&repo.common_dir, &tree.path);
                self.worktrees.get(&key).and_then(|state| state.last.as_ref()).is_none()
            }),
        })
    }

    fn next_wait(&self, now: Instant) -> Duration {
        let mut wait = Duration::from_secs(1);
        let mut deadline = |value: Instant| {
            wait = wait.min(value.saturating_duration_since(now));
        };
        if let Some(last) = self.last_published {
            deadline(last + Duration::from_secs(1));
        }
        deadline(self.next_config_read);
        deadline(self.next_reconcile);
        if let Some(due) = self.pending.due { deadline(due); }
        if let Some(due) = self.watch.next_retry() { deadline(due); }
        if self.jobs.len() < MAX_JOBS {
            if self.launch_directory.is_some() && self.launch_job.is_none() { deadline(self.launch_due); }
            if self.discovery_jobs.len() < MAX_DISCOVERY_JOBS {
                for (common, repo) in &self.repos {
                    if repo.discovery_job.is_none() && !self.discovery_jobs.contains_key(common) {
                        deadline(repo.next_discovery);
                    }
                }
            }
            if self.active_scan_jobs() < MAX_SCAN_JOBS {
                for (key, state) in &self.worktrees {
                    if state.in_flight.is_none() && !state.discovery_error && !self.scan_jobs.contains_key(key)
                        && self.repos.get(&key.common).is_some_and(|repo| matches!(repo.discovery, DiscoveryState::Ready(_))) {
                        deadline(state.due);
                    }
                }
            }
        }
        wait
    }

    fn shutdown(&mut self) {
        self.cancel.cancel();
        for job in self.jobs.values() { job.cancel.cancel(); }
        let jobs = std::mem::take(&mut self.jobs);
        for (_, job) in jobs { let _ = job.handle.join(); }
    }
}

fn display_path(common: &Path) -> PathBuf {
    common.parent().filter(|_| common.file_name().is_some_and(|name| name == ".git"))
        .unwrap_or(common).to_path_buf()
}

fn is_not_git(error: &Error) -> bool {
    error.kind == ErrorKind::Git && error.message.starts_with("fatal: not a git repository")
}

fn worker(service: Service, launch_directory: Option<PathBuf>, cancel: Cancel,
    latest: Arc<Mutex<Option<Arc<MonitorUpdate>>>>) {
    Coordinator::new(service, launch_directory, cancel, latest).run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Counts, Head};

    fn stats(value: u64) -> Result<WorktreeStats, Error> {
        Ok(WorktreeStats { head: Head::Branch(format!("branch-{value}")), counts: Counts {
            source: crate::LineCounts { additions: value, deletions: 0 },
            ..Counts::default()
        } })
    }

    #[test]
    fn worktree_backoffs_are_independent_and_reset_only_on_change() {
        let mut first = WorktreeBackoff::default();
        let mut second = WorktreeBackoff::default();
        let first_result = stats(1);
        let second_result = stats(2);
        assert_eq!(first.observe(&first_result), Duration::from_secs(5));
        assert_eq!(second.observe(&second_result), Duration::from_secs(5));
        assert_eq!(first.observe(&first_result), Duration::from_secs(10));
        assert_eq!(second.observe(&second_result), Duration::from_secs(10));
        assert_eq!(first.observe(&stats(3)), Duration::from_secs(5));
        assert_eq!(second.observe(&second_result), Duration::from_secs(20));
    }

    #[test]
    fn polling_time_clips_old_work_and_counts_active_overlaps() {
        let now = Instant::now();
        let mut polling = PollingTime::default();
        let old_start = now.checked_sub(Duration::from_secs(70)).unwrap();
        polling.record(old_start, old_start + Duration::from_secs(5));
        let clipped_start = now.checked_sub(Duration::from_secs(70)).unwrap();
        let clipped_end = now.checked_sub(Duration::from_secs(50)).unwrap();
        polling.record(clipped_start, clipped_end);
        let active = now.checked_sub(Duration::from_secs(3)).unwrap();
        assert_eq!(polling.total(now, &[(active, None)]), Duration::from_secs(13));
        assert_eq!(polling.total(now, &[(active, Some(now - Duration::from_secs(1)))]), Duration::from_secs(12));
        assert_eq!(polling.total(now + Duration::from_secs(61), &[]), Duration::ZERO);
    }

    #[cfg(unix)]
    #[test]
    fn worktree_keys_preserve_backslashes_on_unix() {
        assert_ne!(normalize(Path::new("/tmp/a\\one")), normalize(Path::new("/tmp/a/one")));
    }

    #[cfg(unix)]
    #[test]
    fn worktree_keys_preserve_non_utf8_names_on_unix() {
        use std::os::unix::ffi::OsStrExt;
        let first = Path::new(std::ffi::OsStr::from_bytes(b"/tmp/tree-\xff"));
        let second = Path::new(std::ffi::OsStr::from_bytes(b"/tmp/tree-\xfe"));
        assert_ne!(normalize(first), normalize(second));
        assert_eq!(normalize(first).as_os_str().as_bytes(), b"/tmp/tree-\xff");
    }

    #[test]
    fn latest_publishes_ready_worktree_while_another_scan_is_active() {
        let latest = Arc::new(Mutex::new(None));
        let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest.clone());
        let now = Instant::now();
        let common = PathBuf::from("repo/.git");
        let fast = PathBuf::from("repo/fast");
        let slow = PathBuf::from("repo/slow");
        let fast_key = WorktreeKey::new(&common, &fast);
        let slow_key = WorktreeKey::new(&common, &slow);
        let slow_job = coordinator.fresh_job();
        let slow_cancel = Cancel::new();
        let slow_timing = Arc::new(Mutex::new(JobTiming::default()));
        let worker_cancel = slow_cancel.clone();
        let worker_timing = slow_timing.clone();
        let timing_observer = slow_timing.clone();
        let handle = std::thread::spawn(move || {
            mark_job_started(&worker_timing);
            worker_cancel.wait(Duration::from_secs(1));
            mark_job_ended(&worker_timing);
        });
        coordinator.jobs.insert(slow_job, Job { handle, cancel: slow_cancel, timing: slow_timing });
        coordinator.scan_jobs.insert(slow_key.clone(), slow_job);
        coordinator.repo_order.push(normalize(&common));
        coordinator.repos.insert(normalize(&common), RepoState {
            common_dir: common.clone(),
            path: PathBuf::from("repo"),
            generation: 1,
            next_discovery: now + Duration::from_secs(1),
            discovery_job: None,
            revision: 0,
            discovery: DiscoveryState::Ready(git::DiscoveredRepo {
                path: PathBuf::from("repo"),
                trees: vec![
                    git::DiscoveredTree { path: fast.clone(), error: None },
                    git::DiscoveredTree { path: slow.clone(), error: None },
                ],
            }),
            worktrees: vec![fast_key.clone(), slow_key.clone()],
        });
        coordinator.worktrees.insert(fast_key, WorktreeState {
            path: fast,
            last: Some(stats(1)),
            discovery_error: false,
            backoff: WorktreeBackoff::default(),
            due: now + Duration::from_secs(1),
            in_flight: None,
            revision: 0,
            generation: 2,
        });
        coordinator.worktrees.insert(slow_key, WorktreeState {
            path: slow,
            last: None,
            discovery_error: false,
            backoff: WorktreeBackoff::default(),
            due: now,
            in_flight: Some(slow_job),
            revision: 0,
            generation: 3,
        });

        while timing_observer.lock().unwrap().started.is_none() { std::thread::yield_now(); }
        coordinator.publish();
        let monitor = Monitor { cancel: Cancel::new(), latest, worker: None };
        let update = monitor.latest().unwrap();
        assert_eq!(update.snapshot.repos.len(), 1);
        assert_eq!(update.snapshot.repos[0].worktrees.as_ref().unwrap().len(), 1);
        assert_eq!(update.snapshot.repos[0].worktrees.as_ref().unwrap()[0].path, PathBuf::from("repo/fast"));
        assert!(update.polling_time_last_minute > Duration::ZERO);
        drop(monitor);
        coordinator.shutdown();
    }

    #[test]
    fn idle_publication_reuses_snapshot_arc() {
        let latest = Arc::new(Mutex::new(None));
        let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest.clone());
        coordinator.publish();
        let first = latest.lock().unwrap().clone().unwrap();
        coordinator.dirty = false;
        coordinator.last_published = Some(Instant::now() - Duration::from_secs(2));
        coordinator.publish();
        let second = latest.lock().unwrap().clone().unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        assert!(Arc::ptr_eq(&first.snapshot, &second.snapshot));
    }

    #[test]
    fn stale_completion_cannot_update_a_readded_worktree() {
        let latest = Arc::new(Mutex::new(None));
        let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
        let key = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo"));
        coordinator.worktrees.insert(key.clone(), WorktreeState {
            path: PathBuf::from("repo"),
            last: None,
            discovery_error: false,
            backoff: WorktreeBackoff::default(),
            due: Instant::now(),
            in_flight: Some(2),
            revision: 0,
            generation: 2,
        });
        coordinator.complete(Completion { id: 1, kind: CompletionKind::Scan {
            key: key.clone(), generation: 1, revision: 0, result: stats(1),
        } }, Instant::now());
        let state = coordinator.worktrees.get(&key).unwrap();
        assert!(state.last.is_none());
        assert_eq!(state.in_flight, Some(2));
    }

    #[test]
    fn pending_worktree_invalidation_survives_an_in_flight_completion() {
        let latest = Arc::new(Mutex::new(None));
        let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
        let now = Instant::now();
        let key = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo"));
        coordinator.worktrees.insert(key.clone(), WorktreeState {
            path: PathBuf::from("repo"),
            last: None,
            discovery_error: false,
            backoff: WorktreeBackoff::default(),
            due: now,
            in_flight: Some(2),
            revision: 0,
            generation: 1,
        });
        let mut batch = watch::WatchBatch::default();
        batch.worktrees.insert(key.clone());
        coordinator.enqueue_watch(batch, now);
        coordinator.complete(Completion { id: 2, kind: CompletionKind::Scan {
            key: key.clone(), generation: 1, revision: 0, result: stats(1),
        } }, now + Duration::from_millis(1));
        let state = coordinator.worktrees.get(&key).unwrap();
        assert!(state.last.is_none());
        assert!(state.due >= now + DEBOUNCE);
        assert_eq!(state.in_flight, None);
    }

    #[test]
    fn pending_discovery_invalidation_survives_an_in_flight_completion() {
        let latest = Arc::new(Mutex::new(None));
        let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
        let now = Instant::now();
        let common = PathBuf::from("repo/.git");
        let common_key = normalize(&common);
        coordinator.repo_order.push(common_key.clone());
        coordinator.repos.insert(common_key.clone(), RepoState {
            common_dir: common.clone(),
            path: PathBuf::from("repo"),
            generation: 1,
            next_discovery: now,
            discovery_job: Some(2),
            revision: 0,
            discovery: DiscoveryState::Pending,
            worktrees: Vec::new(),
        });
        coordinator.discovery_jobs.insert(common_key.clone(), 2);
        let mut batch = watch::WatchBatch::default();
        batch.discoveries.insert(common_key.clone());
        coordinator.enqueue_watch(batch, now);
        coordinator.complete(Completion { id: 2, kind: CompletionKind::Discovery {
            common: common_key.clone(), generation: 1, revision: 0,
            result: Ok(git::DiscoveredRepo { path: PathBuf::from("repo"), trees: Vec::new() }),
        } }, now + Duration::from_millis(1));
        let repo = coordinator.repos.get(&common_key).unwrap();
        assert!(matches!(repo.discovery, DiscoveryState::Pending));
        assert!(repo.next_discovery >= now + DEBOUNCE);
        assert_eq!(repo.discovery_job, None);
    }

    #[test]
    fn safety_reconcile_makes_discovery_and_scan_due_without_losing_revisions() {
        let latest = Arc::new(Mutex::new(None));
        let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
        let now = Instant::now();
        let common = PathBuf::from("repo/.git");
        let common_key = normalize(&common);
        let path = PathBuf::from("repo");
        let key = WorktreeKey::new(&common, &path);
        coordinator.repo_order.push(common_key.clone());
        coordinator.repos.insert(common_key.clone(), RepoState {
            common_dir: common,
            path: path.clone(),
            generation: 1,
            next_discovery: now + Duration::from_secs(30),
            discovery_job: None,
            revision: 7,
            discovery: DiscoveryState::Ready(git::DiscoveredRepo {
                path: path.clone(),
                trees: vec![git::DiscoveredTree { path: path.clone(), error: None }],
            }),
            worktrees: vec![key.clone()],
        });
        coordinator.worktrees.insert(key.clone(), WorktreeState {
            path,
            last: Some(stats(1)),
            discovery_error: false,
            backoff: WorktreeBackoff::default(),
            due: now + Duration::from_secs(30),
            in_flight: None,
            revision: 11,
            generation: 2,
        });
        coordinator.pending.discoveries.insert(common_key.clone());
        coordinator.pending.worktrees.insert(key.clone());
        coordinator.next_reconcile = now;
        coordinator.reconcile(now);
        assert_eq!(coordinator.repos[&common_key].next_discovery, now);
        assert_eq!(coordinator.worktrees[&key].due, now);
        assert_eq!(coordinator.repos[&common_key].revision, 7);
        assert_eq!(coordinator.worktrees[&key].revision, 11);
        assert!(coordinator.pending.discoveries.contains(&common_key));
        assert!(coordinator.pending.worktrees.contains(&key));
    }

    #[test]
    fn failed_full_watch_reconcile_honors_a_retry_deadline() {
        let latest = Arc::new(Mutex::new(None));
        let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
        coordinator.watch.fail_reconcile_for_test();
        let now = Instant::now();
        coordinator.reconcile_watches(now);
        let retry = coordinator.watch.next_retry().unwrap();
        assert!(!coordinator.watch_dirty);
        coordinator.reconcile_watches(now + Duration::from_secs(1));
        assert_eq!(coordinator.watch.next_retry(), Some(retry));
        coordinator.reconcile_watches(retry);
        assert!(coordinator.watch.next_retry().unwrap() > retry);
    }

    #[test]
    fn changing_registrations_keeps_an_existing_worktree_timer() {
        let root = std::env::current_dir().unwrap();
        let first = root.join("fixture-a/.git");
        let second = root.join("fixture-b/.git");
        let latest = Arc::new(Mutex::new(None));
        let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
        let first_bytes = serde_json::to_vec(&serde_json::json!({ "repos": [first] })).unwrap();
        let second_bytes = serde_json::to_vec(&serde_json::json!({ "repos": [first, second] })).unwrap();
        assert!(coordinator.registrations.reload(Ok(first_bytes)));
        let now = Instant::now();
        coordinator.refresh_repositories(now);
        let key = WorktreeKey::new(&root.join("fixture-a/.git"), &root.join("fixture-a"));
        let due = now + Duration::from_secs(42);
        coordinator.worktrees.insert(key, WorktreeState {
            path: root.join("fixture-a"),
            last: Some(stats(1)),
            discovery_error: false,
            backoff: WorktreeBackoff::default(),
            due,
            in_flight: None,
            revision: 0,
            generation: 9,
        });
        assert!(coordinator.registrations.reload(Ok(second_bytes)));
        coordinator.refresh_repositories(now + Duration::from_secs(1));
        let state = coordinator.worktrees.values().next().unwrap();
        assert_eq!(state.due, due);
        assert_eq!(state.generation, 9);
    }

    #[test]
    fn blocked_overdue_jobs_do_not_spin_the_coordinator() {
        let latest = Arc::new(Mutex::new(None));
        let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
        let now = Instant::now();
        coordinator.next_config_read = now + Duration::from_secs(5);
        coordinator.next_reconcile = now + Duration::from_secs(300);
        coordinator.last_published = Some(now);
        let common = PathBuf::from("repo/.git");
        let key = WorktreeKey::new(&common, Path::new("repo"));
        coordinator.repos.insert(key.common.clone(), RepoState {
            common_dir: common,
            path: PathBuf::from("repo"),
            generation: 1,
            next_discovery: now + Duration::from_secs(5),
            discovery_job: None,
            revision: 0,
            discovery: DiscoveryState::Failed(Error::new(ErrorKind::Io, "Missing repository")),
            worktrees: vec![key.clone()],
        });
        coordinator.worktrees.insert(key.clone(), WorktreeState {
            path: PathBuf::from("repo"),
            last: Some(stats(1)),
            discovery_error: false,
            backoff: WorktreeBackoff::default(),
            due: now,
            in_flight: None,
            revision: 0,
            generation: 2,
        });
        assert_eq!(coordinator.next_wait(now), Duration::from_secs(1));
        let repo = coordinator.repos.get_mut(&key.common).unwrap();
        repo.discovery = DiscoveryState::Ready(git::DiscoveredRepo {
            path: PathBuf::from("repo"),
            trees: vec![git::DiscoveredTree { path: PathBuf::from("repo"), error: None }],
        });
        coordinator.scan_jobs.insert(key.clone(), 9);
        assert_eq!(coordinator.next_wait(now), Duration::from_secs(1));
        coordinator.repos.get_mut(&key.common).unwrap().next_discovery = now;
        coordinator.discovery_jobs.insert(key.common.clone(), 10);
        assert_eq!(coordinator.next_wait(now), Duration::from_secs(1));
    }

    #[test]
    fn overdue_jobs_are_fair_and_discovery_cannot_fill_every_slot() {
        let latest = Arc::new(Mutex::new(None));
        let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
        let now = Instant::now();
        let common = PathBuf::from("repo/.git");
        let paths: Vec<_> = (1..=5).map(|index| PathBuf::from(format!("repo/tree-{index}"))).collect();
        let keys: Vec<_> = paths.iter().map(|path| WorktreeKey::new(&common, path)).collect();
        let trees = paths.iter().map(|path| git::DiscoveredTree { path: path.clone(), error: None }).collect();
        coordinator.repo_order.push(normalize(&common));
        coordinator.repos.insert(normalize(&common), RepoState {
            common_dir: common.clone(),
            path: PathBuf::from("repo"),
            generation: 1,
            next_discovery: now - Duration::from_secs(2),
            discovery_job: None,
            revision: 0,
            discovery: DiscoveryState::Ready(git::DiscoveredRepo { path: PathBuf::from("repo"), trees }),
            worktrees: keys.clone(),
        });
        for (index, (key, path)) in keys.iter().zip(paths.iter()).enumerate() {
            coordinator.worktrees.insert(key.clone(), WorktreeState {
                path: path.clone(),
                last: Some(stats(index as u64)),
                discovery_error: false,
                backoff: WorktreeBackoff::default(),
                due: now - Duration::from_secs(index as u64 + 1),
                in_flight: None,
                revision: 0,
                generation: index as u64 + 2,
            });
        }
        for (name, age) in [("repo-b/.git", 1), ("repo-c/.git", 3)] {
            let common = PathBuf::from(name);
            let key = normalize(&common);
            coordinator.repo_order.push(key.clone());
            coordinator.repos.insert(key, RepoState {
                common_dir: common,
                path: PathBuf::from(name),
                generation: 10,
                next_discovery: now - Duration::from_secs(age),
                discovery_job: None,
                revision: 0,
                discovery: DiscoveryState::Ready(git::DiscoveredRepo { path: PathBuf::from(name), trees: Vec::new() }),
                worktrees: Vec::new(),
            });
        }
        coordinator.schedule(now);
        assert_eq!(coordinator.jobs.len(), 4);
        assert_eq!(coordinator.discovery_jobs.len(), 2);
        assert_eq!(coordinator.scan_jobs.len(), 2);
        assert!(coordinator.discovery_jobs.contains_key(&normalize(Path::new("repo-c/.git"))));
        assert!(!coordinator.discovery_jobs.contains_key(&normalize(Path::new("repo-b/.git"))));
        assert!(coordinator.scan_jobs.contains_key(&keys[4]));
        assert!(coordinator.scan_jobs.contains_key(&keys[3]));
        assert!(!coordinator.scan_jobs.contains_key(&keys[0]));
        coordinator.shutdown();
    }
}
