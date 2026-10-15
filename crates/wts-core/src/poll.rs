use crate::{config, git, watch, Cancel, Error, ErrorKind, Service, Snapshot, WorktreeStats};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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
            Coordinator::new(service, launch_directory, worker_cancel, worker_latest).run();
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
struct PollingTime {
    completed: Vec<(Instant, Instant)>,
}

impl PollingTime {
    fn record(&mut self, start: Instant, end: Instant) {
        self.completed.push((start, end));
    }

    fn total(&mut self, now: Instant) -> Duration {
        let window = now.checked_sub(Duration::from_secs(60)).unwrap_or(now);
        self.completed.retain(|(_, end)| *end > window);
        self.completed.iter().map(|(start, end)| {
            end.min(&now).saturating_duration_since(*start.max(&window))
        }).sum()
    }
}

struct WorktreeState {
    path: PathBuf,
    last: Option<Result<WorktreeStats, Error>>,
}

struct RepoState {
    common_dir: PathBuf,
    path: PathBuf,
    discovered: Option<Result<git::DiscoveredRepo, Error>>,
    worktrees: Vec<WorktreeKey>,
    sources: Vec<PathBuf>,
}

const PASS_DELAY: Duration = Duration::from_secs(10);

#[derive(Default)]
struct ScanPasses {
    changed: HashSet<WorktreeKey>,
    next: Option<Instant>,
}

impl ScanPasses {
    fn ready(&self, now: Instant) -> bool { self.next.is_none_or(|next| now >= next) }

    fn finish(&mut self, now: Instant) { self.next = Some(now + PASS_DELAY); }
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
        }
    }

    fn run(mut self) {
        while !self.cancel.is_cancelled() {
            let generation = self.cancel.generation();
            self.reconcile_watches();
            self.receive_watch();
            if self.config_changed {
                self.config_changed = false;
                if self.registrations.reload(config::read(&self.service.config_path)) {
                    self.refresh_repositories();
                }
            }
            if self.passes.ready(Instant::now()) && self.pending() { self.scan_pass(); }
            self.publish();
            let now = Instant::now();
            let mut wait = Duration::from_secs(1);
            if self.pending() {
                if let Some(next) = self.passes.next { wait = wait.min(next.saturating_duration_since(now)); }
            }
            if let Some(retry) = self.watch.next_retry() { wait = wait.min(retry.saturating_duration_since(now)); }
            self.cancel.wait_since(generation, wait);
        }
    }

    fn pending(&self) -> bool {
        self.launch_changed || !self.discoveries.is_empty() || !self.passes.changed.is_empty()
    }

    fn watch_targets(&self) -> watch::WatchTargets {
        let mut global_excludes = git::global_source_candidates();
        for repo in self.repos.values() { global_excludes.extend(repo.sources.iter().cloned()); }
        global_excludes.sort();
        global_excludes.dedup();
        watch::WatchTargets {
            config: self.service.config_path.clone(),
            launch: self.launch_directory.clone(),
            repositories: self.repo_order.iter().map(|key| self.repos[key].common_dir.clone()).collect(),
            worktrees: self.worktrees.iter().map(|(key, state)| (key.clone(), state.path.clone())).collect(),
            global_excludes,
        }
    }

    fn reconcile_watches(&mut self) {
        let now = Instant::now();
        if !self.watch_dirty && !self.watch.next_retry().is_some_and(|retry| now >= retry) { return; }
        let status = self.watch.reconcile(&self.watch_targets(), now);
        self.watch_dirty = false;
        if status.degraded {
            self.monitors_failed = true;
        } else if status.full || self.monitors_failed {
            self.monitors_failed = false;
            self.enqueue(watch::WatchBatch { full: true, ..Default::default() });
        }
    }

    fn receive_watch(&mut self) {
        let batch = self.watch.drain(Instant::now());
        if !batch.is_empty() { self.enqueue(batch); }
    }

    fn enqueue(&mut self, batch: watch::WatchBatch) {
        if batch.config || batch.full { self.config_changed = true; }
        if batch.launch || batch.full { self.launch_changed = true; }
        let discoveries: HashSet<_> = batch.discoveries.iter().map(|common| normalize(common)).collect();
        self.discoveries.extend(discoveries.iter().cloned());
        for common in batch.repositories {
            let common = normalize(&common);
            if discoveries.contains(&common) { continue; }
            if let Some(repo) = self.repos.get(&common) {
                self.passes.changed.extend(repo.worktrees.iter().cloned());
            }
        }
        self.passes.changed.extend(batch.worktrees);
        if batch.global_excludes {
            self.passes.changed.extend(self.worktrees.keys().cloned());
            self.discoveries.extend(self.repo_order.iter().cloned());
        }
        if batch.full { self.discoveries.extend(self.repo_order.iter().cloned()); }
        if batch.topology || batch.full { self.watch_dirty = true; }
    }

    fn refresh_repositories(&mut self) {
        let mut desired = Vec::new();
        let mut seen = HashSet::new();
        for common in self.registrations.repos.iter().chain(self.launch_common.iter()) {
            let key = normalize(common);
            if seen.insert(key.clone()) { desired.push((key, common.clone())); }
        }
        self.repos.retain(|key, _| seen.contains(key));
        self.worktrees.retain(|key, _| seen.contains(&key.common));
        self.passes.changed.retain(|key| seen.contains(&key.common));
        self.discoveries.retain(|key| seen.contains(key));
        self.repo_order = desired.iter().map(|(key, _)| key.clone()).collect();
        for (key, common_dir) in desired {
            if let Some(repo) = self.repos.get_mut(&key) {
                repo.common_dir = common_dir;
            } else {
                self.discoveries.insert(key.clone());
                self.repos.insert(key, RepoState {
                    path: display_path(&common_dir), common_dir,
                    discovered: None, worktrees: Vec::new(), sources: Vec::new(),
                });
            }
        }
        self.watch_dirty = true;
    }

    fn scan_pass(&mut self) {
        let mut worked = false;
        if self.launch_changed {
            self.launch_changed = false;
            if let Some(directory) = &self.launch_directory {
                worked = true;
                let start = Instant::now();
                let result = config::resolve(directory, &self.cancel);
                self.polling_time.record(start, Instant::now());
                match result {
                    Ok(common) => { self.launch_common = Some(common); self.launch_error = None; }
                    Err(error) if is_not_git(&error) => { self.launch_error = None; }
                    Err(error) => { self.launch_error = Some(error); }
                }
                self.refresh_repositories();
            }
        }
        let discoveries = std::mem::take(&mut self.discoveries);
        for common in self.repo_order.clone() {
            if self.cancel.is_cancelled() { return; }
            if !discoveries.contains(&common) { continue; }
            worked = true;
            let start = Instant::now();
            let result = git::discover(&self.repos[&common].common_dir, &self.cancel);
            self.apply_discovery(&common, result);
            self.polling_time.record(start, Instant::now());
        }
        self.reconcile_watches();
        self.receive_watch();
        let changed = std::mem::take(&mut self.passes.changed);
        let keys: Vec<_> = self.repo_order.iter().flat_map(|common| self.repos[common].worktrees.iter())
            .filter(|key| changed.contains(*key)).cloned().collect();
        for key in keys {
            if self.cancel.is_cancelled() { return; }
            if !self.repos.get(&key.common).is_some_and(|repo| matches!(repo.discovered, Some(Ok(_)))) { continue; }
            let Some(state) = self.worktrees.get(&key) else { continue; };
            let path = state.path.clone();
            worked = true;
            let start = Instant::now();
            let result = git::scan_tree(&path, &self.cancel);
            self.polling_time.record(start, Instant::now());
            self.worktrees.get_mut(&key).unwrap().last = Some(result);
            self.publish();
            self.receive_watch();
        }
        if worked { self.passes.finish(Instant::now()); }
    }

    fn apply_discovery(&mut self, common: &Path, result: Result<git::DiscoveredRepo, Error>) {
        let repo = self.repos.get_mut(common).unwrap();
        let recovered = matches!(repo.discovered, Some(Err(_)));
        if let Ok(discovered) = &result {
            repo.path = discovered.path.clone();
            let mut keys = Vec::new();
            for tree in &discovered.trees {
                let key = WorktreeKey::new(&repo.common_dir, &tree.path);
                if keys.contains(&key) { continue; }
                keys.push(key.clone());
                if let Some(state) = self.worktrees.get_mut(&key) {
                    state.path = tree.path.clone();
                    if let Some(error) = &tree.error {
                        state.last = Some(Err(error.clone()));
                    } else if recovered || state.last.as_ref().is_some_and(Result::is_err) {
                        self.passes.changed.insert(key);
                    }
                } else {
                    self.worktrees.insert(key.clone(), WorktreeState {
                        path: tree.path.clone(), last: tree.error.clone().map(Err),
                    });
                    if tree.error.is_none() { self.passes.changed.insert(key); }
                }
            }
            let retained: HashSet<_> = keys.iter().cloned().collect();
            self.worktrees.retain(|key, _| key.common != common || retained.contains(key));
            self.passes.changed.retain(|key| key.common != common || retained.contains(key));
            repo.worktrees = keys;
            if let Some(tree) = discovered.trees.iter().find(|tree| tree.error.is_none()) {
                if let Ok(sources) = git::global_sources(&tree.path, &self.cancel) { repo.sources = sources; }
            }
        }
        repo.discovered = Some(result);
        self.watch_dirty = true;
    }

    fn publish(&mut self) {
        let repos = self.repo_order.iter().filter_map(|common| {
            let repo = &self.repos[common];
            match repo.discovered.as_ref()? {
                Err(error) => Some(crate::RepoEntry {
                    common_dir: repo.common_dir.clone(), path: repo.path.clone(), worktrees: Err(error.clone()),
                }),
                Ok(_) => {
                    let trees: Vec<_> = repo.worktrees.iter().filter_map(|key| {
                        let state = &self.worktrees[key];
                        state.last.as_ref().map(|last| crate::WorktreeEntry { path: state.path.clone(), result: last.clone() })
                    }).collect();
                    if trees.is_empty() && !repo.worktrees.is_empty() { return None; }
                    Some(crate::RepoEntry { common_dir: repo.common_dir.clone(), path: repo.path.clone(), worktrees: Ok(trees) })
                }
            }
        }).collect();
        let snapshot = Snapshot { repos, config_error: self.registrations.error.clone() };
        if snapshot != *self.snapshot { self.snapshot = Arc::new(snapshot); }
        let update = MonitorUpdate {
            snapshot: self.snapshot.clone(), launch_error: self.launch_error.clone(),
            polling_time_last_minute: self.polling_time.total(Instant::now()),
        };
        *self.latest.lock().unwrap_or_else(|error| error.into_inner()) = Some(Arc::new(update));
    }
}

fn display_path(common: &Path) -> PathBuf {
    common.parent().filter(|_| common.file_name().is_some_and(|name| name == ".git"))
        .unwrap_or(common).to_path_buf()
}

fn is_not_git(error: &Error) -> bool {
    error.kind == ErrorKind::Git && error.message.starts_with("fatal: not a git repository")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_wait_ten_seconds_after_completion_without_sliding() {
        let mut passes = ScanPasses::default();
        let now = Instant::now();
        assert!(passes.ready(now));
        passes.finish(now);
        let key = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo"));
        for second in 1..10 {
            passes.changed.insert(key.clone());
            assert!(!passes.ready(now + Duration::from_secs(second)));
        }
        assert!(passes.ready(now + PASS_DELAY));
        assert_eq!(passes.changed.len(), 1);
    }

    #[test]
    fn worktree_events_only_queue_the_changed_worktree() {
        let latest = Arc::new(Mutex::new(None));
        let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
        let first = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo/first"));
        let second = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo/second"));
        let mut batch = watch::WatchBatch::default();
        batch.worktrees.insert(first.clone());
        coordinator.enqueue(batch);
        assert!(coordinator.passes.changed.contains(&first));
        assert!(!coordinator.passes.changed.contains(&second));
    }

    #[test]
    fn taking_a_pass_preserves_new_events_for_the_next_pass() {
        let mut passes = ScanPasses::default();
        let key = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo"));
        passes.changed.insert(key.clone());
        let current = std::mem::take(&mut passes.changed);
        passes.changed.insert(key.clone());
        assert!(current.contains(&key));
        assert!(passes.changed.contains(&key));
    }

    #[test]
    fn monitor_repair_does_not_schedule_unchanged_worktrees() {
        let latest = Arc::new(Mutex::new(None));
        let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
        let key = WorktreeKey::new(Path::new("repo/.git"), Path::new("repo"));
        coordinator.worktrees.insert(key, WorktreeState { path: PathBuf::from("repo"), last: None });
        coordinator.enqueue(watch::WatchBatch { full: true, ..Default::default() });
        assert!(coordinator.passes.changed.is_empty());
    }

    #[test]
    fn discovery_schedules_new_worktrees_once_without_file_events() {
        let directory = tempfile::tempdir().unwrap();
        crate::process::checked(Some(directory.path()), &crate::process::args(&["init", "--quiet"]), &Cancel::new()).unwrap();
        let common_dir = directory.path().join(".git");
        let common = normalize(&common_dir);
        let latest = Arc::new(Mutex::new(None));
        let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
        coordinator.repos.insert(common.clone(), RepoState {
            common_dir, path: directory.path().to_path_buf(), discovered: None,
            worktrees: Vec::new(), sources: Vec::new(),
        });
        let mut discovered = git::DiscoveredRepo {
            path: directory.path().to_path_buf(),
            trees: vec![git::DiscoveredTree { path: directory.path().to_path_buf(), error: None }],
        };
        coordinator.apply_discovery(&common, Ok(discovered.clone()));
        assert_eq!(coordinator.passes.changed.len(), 1);
        coordinator.passes.changed.clear();
        coordinator.apply_discovery(&common, Ok(discovered.clone()));
        assert!(coordinator.passes.changed.is_empty());
        let new_path = directory.path().join("new-worktree");
        discovered.trees.push(git::DiscoveredTree { path: new_path.clone(), error: None });
        coordinator.apply_discovery(&common, Ok(discovered.clone()));
        assert_eq!(coordinator.passes.changed, HashSet::from([WorktreeKey::new(&common, &new_path)]));
        coordinator.passes.changed.clear();
        coordinator.apply_discovery(&common, Ok(discovered));
        assert!(coordinator.passes.changed.is_empty());
    }

    #[test]
    fn polling_time_only_counts_serial_work_inside_the_last_minute() {
        let now = Instant::now();
        let mut polling = PollingTime::default();
        polling.record(now - Duration::from_secs(70), now - Duration::from_secs(55));
        polling.record(now - Duration::from_secs(40), now - Duration::from_secs(35));
        assert_eq!(polling.total(now), Duration::from_secs(10));
    }
}
