#[path = "../filesystem-watchers-recover-after-failures/recovery.rs"]
mod recovery;
use recovery::{add_existing_or_parent, add_recovery, add_recovery_with_mode, recovery_root};

#[path = "routing.rs"]
mod routing;
use routing::{rule_file, topology_event};
#[cfg(test)]
use routing::launch_event;

#[path = "../ignored-directories-do-not-cause-background-scans/policy.rs"]
mod policy;
use policy::EventPolicy;

#[cfg(test)]
use crate::git;
use crate::poll::{normalize, WorktreeKey};
use crate::Cancel;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{mpsc, Arc, RwLock};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

#[path = "../ignored-directories-do-not-cause-background-scans/watch_plan.rs"]
mod watch_plan;
use watch_plan::{fallback_plan, plan_for, WorktreeWatchPlan};

const MAX_DRAIN_EVENTS: usize = 128;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum Target {
    Config(PathBuf),
    GlobalExclude(PathBuf),
    RepositorySource(PathBuf, PathBuf),
    Launch(PathBuf),
    Repository(PathBuf),
    Worktree(WorktreeKey),
    WorktreeMetadata(WorktreeKey, PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Registration {
    mode: RecursiveMode,
    targets: Vec<Target>,
}

#[derive(Default)]
pub(crate) struct WatchBatch {
    pub(crate) config: bool,
    pub(crate) launch: bool,
    pub(crate) repositories: HashSet<PathBuf>,
    pub(crate) discoveries: HashSet<PathBuf>,
    pub(crate) worktrees: HashSet<WorktreeKey>,
    pub(crate) topology: bool,
    pub(crate) full: bool,
    pub(crate) global_excludes: bool,
    pub(crate) plans: HashSet<WorktreeKey>,
    pub(crate) worktree_paths: HashMap<WorktreeKey, HashMap<PathBuf, EventKind>>,
    pub(crate) force_worktrees: HashSet<WorktreeKey>,
}

impl WatchBatch {
    pub(crate) fn is_empty(&self) -> bool {
        !self.config && !self.launch && self.repositories.is_empty() && self.discoveries.is_empty()
            && self.worktrees.is_empty() && !self.topology && !self.full
            && !self.global_excludes
            && self.plans.is_empty()
    }
}

pub(crate) struct WatchTargets {
    pub(crate) config: PathBuf,
    pub(crate) launch: Option<PathBuf>,
    pub(crate) repositories: Vec<PathBuf>,
    pub(crate) worktrees: Vec<(WorktreeKey, PathBuf)>,
    pub(crate) global_excludes: Vec<PathBuf>,
    pub(crate) repository_sources: Vec<(PathBuf, PathBuf)>,
}

pub(crate) struct WatchStatus {
    pub(crate) full: bool,
    pub(crate) degraded: bool,
}

fn forward_event(result: Result<Event, notify::Error>, sender: &Sender<Result<Event, notify::Error>>,
    cancel: &Cancel, policy: &EventPolicy) {
    match result {
        Err(error) => {
            let _ = sender.send(Err(error));
            cancel.wake();
        }
        Ok(event) => {
            let event = if cfg!(windows) { policy.filter(event) } else { Some(event) };
            if let Some(event) = event {
                let _ = sender.send(Ok(event));
                cancel.wake();
            }
        }
    }
}

pub(crate) struct WatchBoundary {
    watcher: Option<RecommendedWatcher>,
    sender: Sender<Result<Event, notify::Error>>,
    receiver: Receiver<Result<Event, notify::Error>>,
    cancel: Cancel,
    event_policy: Arc<RwLock<Arc<EventPolicy>>>,
    roots: HashMap<PathBuf, Registration>,
    plans: HashMap<WorktreeKey, Arc<WorktreeWatchPlan>>,
    plan_dirty: HashSet<WorktreeKey>,
    plan_errors: HashSet<WorktreeKey>,
    retry_due: Option<Instant>,
    force_rebuild: bool,
    degraded: bool,
    #[cfg(test)]
    pub(crate) before_plan: Option<Box<dyn FnMut() + Send>>,
}

impl WatchBoundary {
    pub(crate) fn new(cancel: Cancel) -> Self {
        let (sender, receiver) = mpsc::channel();
        let mut boundary = Self {
            watcher: None,
            sender,
            receiver,
            cancel,
            event_policy: Arc::new(RwLock::new(Arc::new(EventPolicy::default()))),
            roots: HashMap::new(),
            plans: HashMap::new(),
            plan_dirty: HashSet::new(),
            plan_errors: HashSet::new(),
            retry_due: None,
            force_rebuild: false,
            degraded: false,
            #[cfg(test)]
            before_plan: None,
        };
        let now = Instant::now();
        boundary.create_watcher(now);
        boundary
    }

    pub(crate) fn rebuild_plans(&mut self, targets: &WatchTargets, now: Instant,
        eligible: &HashSet<WorktreeKey>) -> WatchStatus {
        if self.force_rebuild || self.watcher.is_none() { self.plan_dirty.extend(self.plans.keys().cloned()); }
        self.refresh_plans(targets, eligible);
        self.reconcile_roots(targets, now)
    }

    pub(crate) fn reconcile(&mut self, targets: &WatchTargets, now: Instant) -> WatchStatus {
        if self.force_rebuild || self.watcher.is_none() { self.plan_dirty.extend(self.plans.keys().cloned()); }
        let desired: HashSet<_> = targets.worktrees.iter().map(|(key, _)| key.clone()).collect();
        self.plans.retain(|key, _| desired.contains(key));
        self.plan_dirty.retain(|key| desired.contains(key));
        self.plan_errors.retain(|key| desired.contains(key));
        for (key, path) in &targets.worktrees {
            if !self.plans.contains_key(key) {
                self.plans.insert(key.clone(), Arc::new(fallback_plan(path)));
                self.plan_dirty.insert(key.clone());
            }
        }
        self.reconcile_roots(targets, now)
    }

    pub(crate) fn has_pending_plans(&self) -> bool {
        !self.plan_dirty.is_empty() || !self.plan_errors.is_empty()
    }

    fn reconcile_roots(&mut self, targets: &WatchTargets, now: Instant) -> WatchStatus {
        self.publish_policy(targets);
        let desired = self.registrations(targets);
        let changed = self.force_rebuild || self.watcher.is_none() || self.roots != desired;
        if !changed && !self.degraded {
            return self.reconcile_status(false, &desired, now, false);
        }
        let mut full = self.rebuild_if_required(now);
        let mut failed = self.watcher.is_none();
        if let Some(watcher) = self.watcher.as_mut() {
            let replacements: Vec<_> = self.roots.iter()
                .filter(|(path, registration)| desired.get(*path)
                    .is_some_and(|desired| desired.mode != registration.mode))
                .map(|(path, _)| path.clone())
                .collect();
            if !replacements.is_empty() {
                full = true;
                self.plan_dirty.extend(self.plans.keys().cloned());
            }
            for path in replacements {
                if watcher.unwatch(&path).is_err() {
                    failed = true;
                }
                self.roots.remove(&path);
            }
            let additions: Vec<_> = desired.iter()
                .filter(|(path, registration)| self.roots.get(*path)
                    .is_none_or(|current| current.mode != registration.mode))
                .map(|(path, registration)| (path.clone(), registration.clone()))
                .collect();
            for (path, registration) in additions {
                match watcher.watch(&path, registration.mode) {
                    Ok(()) => { self.roots.insert(path, registration); }
                    Err(_) => { failed = true; full = true; }
                }
            }
            for (path, registration) in &desired {
                if self.roots.get(path).is_some_and(|current| current.mode == registration.mode) {
                    self.roots.insert(path.clone(), registration.clone());
                }
            }
            let stale: Vec<_> = self.roots.iter()
                .filter(|(path, _)| !desired.contains_key(*path))
                .map(|(path, _)| path.clone())
                .collect();
            for path in stale {
                if watcher.unwatch(&path).is_err() {
                    failed = true;
                }
                self.roots.remove(&path);
            }
        }
        self.reconcile_status(failed, &desired, now, full)
    }

    fn registrations(&self, targets: &WatchTargets) -> HashMap<PathBuf, Registration> {
        registrations_with_plans(targets, &self.plans)
    }

    pub(crate) fn drain(&mut self, now: Instant) -> WatchBatch {
        let mut batch = WatchBatch::default();
        let mut drained = 0;
        while drained < MAX_DRAIN_EVENTS {
            let Ok(result) = self.receiver.try_recv() else { break; };
            drained += 1;
            match result {
                Ok(event) => self.classify(event, &mut batch),
                Err(_) => {
                    batch.full = true;
                    self.request_rebuild(None);
                }
            }
        }
        self.filter_ignored_batch(&mut batch);
        if drained == MAX_DRAIN_EVENTS { self.cancel.wake(); }
        if batch.full {
            self.request_rebuild(Some(now));
        }
        batch
    }



}

fn registrations_with_plans(targets: &WatchTargets,
    plans: &HashMap<WorktreeKey, Arc<WorktreeWatchPlan>>) -> HashMap<PathBuf, Registration> {
    #[cfg(windows)]
    let _ = plans;
    let mut result = HashMap::new();
    let config = absolute(&targets.config);
    let config_mode = if nested_in_worktree(&config, targets) {
        RecursiveMode::NonRecursive
    } else {
        RecursiveMode::Recursive
    };
    add_recovery_with_mode(&mut result, &config, Target::Config(config.clone()), config_mode);
    for exclude in &targets.global_excludes {
        let exclude = absolute(exclude);
        let target = Target::GlobalExclude(exclude.clone());
        let mode = if nested_in_worktree(&exclude, targets) {
            RecursiveMode::NonRecursive
        } else {
            RecursiveMode::Recursive
        };
        add_recovery_with_mode(&mut result, &exclude, target, mode);
    }
    for (common, source) in &targets.repository_sources {
        let source = absolute(source);
        let mode = if nested_in_worktree(&source, targets) {
            RecursiveMode::NonRecursive
        } else { RecursiveMode::Recursive };
        add_recovery_with_mode(&mut result, &source,
            Target::RepositorySource(normalize(&absolute(common)), source.clone()), mode);
    }
    if let Some(launch) = &targets.launch {
        let launch = absolute(launch);
        let target = Target::Launch(launch.clone());
        if launch.is_dir() {
            add_target(&mut result, launch.clone(), RecursiveMode::NonRecursive, target.clone());
        } else {
            let mode = if nested_in_worktree(&launch, targets) {
                RecursiveMode::NonRecursive
            } else {
                RecursiveMode::Recursive
            };
            add_recovery_with_mode(&mut result, &launch, target.clone(), mode);
        }
    }
    for common in &targets.repositories {
        let common = absolute(common);
        let repository = Target::Repository(common.clone());
        add_existing_or_parent(&mut result, &common, RecursiveMode::NonRecursive, repository.clone());
        for directory in [common.join("refs"), common.join("worktrees"), common.join("info")] {
            add_existing_or_parent(&mut result, &directory, RecursiveMode::Recursive, repository.clone());
        }
        let exclude = common.join("info/exclude");
        add_recovery(&mut result, &exclude, Target::RepositorySource(normalize(&common), exclude.clone()));
    }
    for (key, path) in &targets.worktrees {
        let path = absolute(path);
        add_recovery(&mut result, &path, Target::Worktree(key.clone()));
        #[cfg(windows)]
        if path.is_dir() {
            add_target(&mut result, path.clone(), RecursiveMode::Recursive, Target::Worktree(key.clone()));
        }
        #[cfg(not(windows))]
        let plan = plans.get(key).cloned().unwrap_or_else(|| Arc::new(fallback_plan(&path)));
        #[cfg(not(windows))]
        for root in &plan.recursive_roots {
            add_target(&mut result, root.clone(), RecursiveMode::Recursive, Target::Worktree(key.clone()));
        }
        #[cfg(not(windows))]
        for gate in &plan.directory_gates {
            add_existing_or_parent(&mut result, gate, RecursiveMode::NonRecursive,
                Target::Worktree(key.clone()));
        }
    }
    add_metadata_targets(&mut result, targets);
    #[cfg(windows)]
    merge_windows_registrations(&mut result);
    for registration in result.values_mut() {
        registration.targets.sort();
        registration.targets.dedup();
    }
    result
}

#[cfg(windows)]
fn merge_windows_registrations(result: &mut HashMap<PathBuf, Registration>) {
    loop {
        let recursive: Vec<_> = result.iter()
            .filter(|(_, registration)| registration.mode == RecursiveMode::Recursive)
            .map(|(path, _)| path.clone()).collect();
        let nested = result.keys().filter_map(|path| {
            recursive.iter().filter(|root| *root != path && path.starts_with(*root))
                .max_by_key(|root| root.components().count())
                .map(|root| (path.clone(), root.clone()))
        }).next();
        let Some((child, root)) = nested else { break; };
        let Some(registration) = result.remove(&child) else { continue; };
        if let Some(parent) = result.get_mut(&root) {
            parent.targets.extend(registration.targets);
        } else {
            result.insert(child, registration);
        }
    }
}

fn add_metadata_targets(result: &mut HashMap<PathBuf, Registration>, targets: &WatchTargets) {
    for (key, path) in &targets.worktrees {
        let marker = path.join(".git");
        let directory = if marker.is_dir() { Some(marker) } else {
            std::fs::read_to_string(&marker).ok().and_then(|text| {
                text.trim().strip_prefix("gitdir: ").map(|directory| {
                    let directory = PathBuf::from(directory);
                    if directory.is_absolute() { directory } else { path.join(directory) }
                })
            })
        };
        if let Some(directory) = directory {
            for registration in result.values_mut() {
                if registration.targets.iter().any(|target| matches!(target,
                    Target::Repository(common) if normalize(common) == key.common)) {
                    registration.targets.push(Target::WorktreeMetadata(key.clone(), normalize(&absolute(&directory))));
                }
            }
        }
    }
}




fn nested_in_worktree(path: &Path, targets: &WatchTargets) -> bool {
    let path = normalize(&absolute(path));
    targets.worktrees.iter().any(|(_, tree)| {
        let tree = normalize(&absolute(tree));
        path == tree || path.starts_with(&tree)
    })
}

fn add_target(result: &mut HashMap<PathBuf, Registration>, path: PathBuf,
    mode: RecursiveMode, target: Target) {
    let path = normalize(&absolute(&path));
    let registration = result.entry(path).or_insert_with(|| Registration {
        mode: RecursiveMode::NonRecursive,
        targets: Vec::new(),
    });
    if mode == RecursiveMode::Recursive {
        registration.mode = RecursiveMode::Recursive;
    }
    registration.targets.push(target);
}



fn absolute(path: &Path) -> PathBuf {
    let path = if path.is_absolute() { path.to_path_buf() } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => { result.pop(); }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

#[cfg(test)]
#[path = "watch.test.rs"]
mod tests;
