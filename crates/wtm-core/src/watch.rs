use crate::git;
use crate::poll::{normalize, WorktreeKey};
use crate::Cancel;
use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{mpsc, Arc, RwLock};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

#[path = "watch_plan.rs"]
mod watch_plan;
use watch_plan::{fallback_plan, plan_for, WorktreeWatchPlan};

const MAX_DRAIN_EVENTS: usize = 128;
pub(crate) const WATCH_RETRY: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum Target {
    Config(PathBuf),
    GlobalExclude(PathBuf),
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
}

pub(crate) struct WatchStatus {
    pub(crate) full: bool,
    pub(crate) degraded: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct EventPolicy {
    worktrees: Vec<WorktreeEventPolicy>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorktreeEventPolicy {
    key: WorktreeKey,
    root: PathBuf,
    plan: Arc<WorktreeWatchPlan>,
    protected: Vec<PathBuf>,
}

impl EventPolicy {
    fn filter(&self, mut event: Event) -> Option<Event> {
        if event.need_rescan() || event.paths.is_empty() {
            return Some(event);
        }
        event.paths.retain(|path| self.allows(path, &event.kind));
        (!event.paths.is_empty()).then_some(event)
    }

    fn allows(&self, path: &Path, kind: &EventKind) -> bool {
        let path = normalize(&absolute(path));
        let mut matching = self.worktrees.iter().filter(|worktree| path.starts_with(&worktree.root)).peekable();
        matching.peek().is_none() || matching.any(|worktree| !ignored_by_plan(&path, kind, worktree))
    }
}

fn ignored_by_plan(path: &Path, kind: &EventKind, worktree: &WorktreeEventPolicy) -> bool {
    if worktree.protected.iter().any(|protected| {
        path == protected || path.starts_with(protected)
            || topology_event(kind) && protected.starts_with(path)
    }) {
        return false;
    }
    if !worktree.plan.ignored_directories.iter().any(|directory| {
        path == directory || path.starts_with(directory)
    }) {
        return false;
    }
    if worktree.plan.tracked_files.contains(path)
        || topology_event(kind) && worktree.plan.tracked_directories.contains(path)
        || rule_file(path) && path.parent().is_some_and(|parent| worktree.plan.tracked_directories.contains(parent)) {
        return false;
    }
    true
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

#[derive(Clone, Copy, Eq, PartialEq)]
enum PlanPhase { Deferred, Ready }

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
    plan_phase: PlanPhase,
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
            plan_phase: PlanPhase::Ready,
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

    pub(crate) fn reconcile(&mut self, targets: &WatchTargets, now: Instant) -> WatchStatus {
        self.plan_phase = PlanPhase::Ready;
        if self.force_rebuild || self.watcher.is_none() { self.plan_dirty.extend(self.plans.keys().cloned()); }
        self.refresh_plans(targets);
        self.reconcile_roots(targets, now)
    }

    pub(crate) fn prepare(&mut self, targets: &WatchTargets, now: Instant) -> WatchStatus {
        self.plan_phase = PlanPhase::Deferred;
        if self.force_rebuild || self.watcher.is_none() { self.plan_dirty.extend(self.plans.keys().cloned()); }
        for (key, path) in &targets.worktrees {
            if !self.plans.contains_key(key) {
                self.plans.insert(key.clone(), Arc::new(fallback_plan(path)));
                self.plan_dirty.insert(key.clone());
            }
        }
        self.reconcile_roots(targets, now)
    }

    fn reconcile_roots(&mut self, targets: &WatchTargets, now: Instant) -> WatchStatus {
        self.publish_policy(targets);
        let desired = self.registrations(targets);
        let planning_failed = !self.plan_errors.is_empty();
        let changed = self.force_rebuild || self.watcher.is_none() || self.roots != desired || planning_failed;
        if !changed && !self.degraded {
            return WatchStatus { full: false, degraded: false };
        }
        let mut full = self.force_rebuild || self.watcher.is_none();
        if self.force_rebuild {
            self.watcher = None;
            self.roots.clear();
            self.create_watcher(now);
        } else if self.watcher.is_none() {
            self.create_watcher(now);
        }
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
        if failed { full = true; }
        self.force_rebuild = failed;
        self.degraded = failed || self.watcher.is_none() || self.roots != desired;
        if self.degraded || planning_failed {
            self.retry_due = Some(now + WATCH_RETRY);
        } else {
            self.retry_due = None;
            self.force_rebuild = false;
        }
        WatchStatus { full, degraded: self.degraded }
    }

    fn refresh_plans(&mut self, targets: &WatchTargets) {
        let desired: HashSet<_> = targets.worktrees.iter().map(|(key, _)| key.clone()).collect();
        self.plans.retain(|key, _| desired.contains(key));
        self.plan_dirty.retain(|key| desired.contains(key));
        self.plan_errors.retain(|key| desired.contains(key));
        for (key, path) in &targets.worktrees {
            if self.plans.contains_key(key) && !self.plan_dirty.contains(key) && !self.plan_errors.contains(key) { continue; }
            self.refresh_plan(key, path);
        }
    }

    fn refresh_plan(&mut self, key: &WorktreeKey, path: &Path) {
        #[cfg(test)]
        if let Some(before_plan) = &mut self.before_plan { before_plan(); }
        let plan = git::watch_paths(path, &self.cancel).map(|paths| plan_for(path, &paths));
        match plan {
            Ok(plan) => {
                self.plans.insert(key.clone(), Arc::new(plan));
                self.plan_errors.remove(key);
            }
            Err(_) => {
                self.plans.insert(key.clone(), Arc::new(fallback_plan(path)));
                self.plan_errors.insert(key.clone());
            }
        }
        self.plan_dirty.remove(key);
    }

    fn refresh_dirty_plans(&mut self) {
        if self.plan_phase == PlanPhase::Deferred { return; }
        let dirty: Vec<_> = self.plan_dirty.iter().cloned().collect();
        if dirty.is_empty() { return; }
        for key in dirty { self.refresh_plan(&key, &key.path); }
        self.publish_plan_policy();
    }

    fn registrations(&self, targets: &WatchTargets) -> HashMap<PathBuf, Registration> {
        registrations_with_plans(targets, &self.plans)
    }

    fn publish_policy(&self, targets: &WatchTargets) {
        let mut worktrees = Vec::new();
        for (key, path) in &targets.worktrees {
            let Some(plan) = self.plans.get(key) else { continue; };
            let mut protected = vec![normalize(&absolute(&path.join(".git"))), key.common.clone()];
            protected.extend(targets.repositories.iter().cloned());
            protected.extend(targets.global_excludes.iter().cloned());
            protected.push(targets.config.clone());
            if let Some(launch) = &targets.launch {
                protected.extend(launch.ancestors().map(|ancestor| ancestor.join(".git")));
            }
            protected = protected.into_iter().map(|path| normalize(&absolute(&path))).collect();
            protected.sort();
            protected.dedup();
            worktrees.push(WorktreeEventPolicy {
                key: key.clone(), root: normalize(&absolute(path)), plan: plan.clone(), protected,
            });
        }
        self.replace_policy(EventPolicy { worktrees });
    }

    fn publish_plan_policy(&self) {
        let current = self.event_policy.read().map(|policy| policy.as_ref().clone()).unwrap_or_default();
        let worktrees = current.worktrees.into_iter().filter_map(|mut worktree| {
            let plan = self.plans.get(&worktree.key)?.clone();
            worktree.plan = plan;
            Some(worktree)
        }).collect();
        self.replace_policy(EventPolicy { worktrees });
    }

    fn replace_policy(&self, policy: EventPolicy) {
        if let Ok(mut current) = self.event_policy.write() {
            *current = Arc::new(policy);
        }
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
                    self.force_rebuild = true;
                    self.degraded = true;
                }
            }
        }
        self.refresh_dirty_plans();
        self.filter_ignored_batch(&mut batch);
        if drained == MAX_DRAIN_EVENTS { self.cancel.wake(); }
        if batch.full {
            self.force_rebuild = true;
            self.degraded = true;
            self.retry_due = Some(now);
        }
        batch
    }

    fn filter_ignored_batch(&self, batch: &mut WatchBatch) {
        let keys: Vec<_> = batch.worktrees.iter().cloned().collect();
        for key in keys {
            if batch.force_worktrees.contains(&key) {
                continue;
            }
            let allowed = batch.worktree_paths.get(&key).is_some_and(|paths| {
                paths.iter().any(|(path, kind)| !self.ignored_event(path, &key, kind))
            });
            if !allowed {
                batch.worktrees.remove(&key);
                batch.worktree_paths.remove(&key);
                batch.force_worktrees.remove(&key);
            }
        }
    }

    pub(crate) fn next_retry(&self) -> Option<Instant> { self.retry_due }

    fn create_watcher(&mut self, now: Instant) {
        let sender = self.sender.clone();
        let cancel = self.cancel.clone();
        let event_policy = self.event_policy.clone();
        let watcher = RecommendedWatcher::new(move |result| {
            let policy = event_policy.read().map(|policy| policy.clone()).unwrap_or_default();
            forward_event(result, &sender, &cancel, &policy);
        }, notify::Config::default());
        match watcher {
            Ok(watcher) => {
                self.watcher = Some(watcher);
                self.degraded = false;
                self.retry_due = None;
            }
            Err(_) => {
                self.watcher = None;
                self.degraded = true;
                self.retry_due = Some(now + WATCH_RETRY);
            }
        }
    }

    fn classify(&mut self, event: Event, batch: &mut WatchBatch) {
        if event.need_rescan() {
            batch.full = true;
        }
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        if event.paths.is_empty() {
            batch.full = true;
            return;
        }
        let mut invalid_roots = HashSet::new();
        for path in event.paths {
            let path = absolute(&path);
            let path = normalize(&path);
            if path.extension().is_some_and(|extension| extension == "lock")
                && self.roots.values().flat_map(|registration| &registration.targets)
                    .any(|target| matches!(target, Target::Repository(common) if path.starts_with(normalize(common)))) {
                continue;
            }
            if topology_event(&event.kind) && self.roots.contains_key(&path) {
                invalid_roots.insert(path.clone());
                batch.topology = true;
            }
            let mut repositories = HashSet::new();
            let metadata: HashSet<_> = self.roots.values()
                .flat_map(|registration| &registration.targets)
                .filter_map(|target| match target {
                    Target::WorktreeMetadata(key, directory) if metadata_event(&path, directory, &key.common, &event.kind) => Some(key.clone()),
                    _ => None,
                }).collect();
            for registration in self.roots.values() {
                for target in &registration.targets {
                    if let Target::Repository(common) = target {
                        if path.starts_with(normalize(common))
                            || normalize(common).starts_with(&path) && topology_event(&event.kind) {
                            let key = normalize(common);
                            repositories.insert(key.clone());
                            let objects = key.join("objects");
                            let object_write = path.starts_with(&objects)
                                && !path.starts_with(objects.join("info"))
                                && (path != objects || !topology_event(&event.kind))
                                && matches!(event.kind, EventKind::Create(_)
                                    | EventKind::Modify(ModifyKind::Data(_)
                                        | ModifyKind::Metadata(_) | ModifyKind::Any));
                            let directory_write = matches!(event.kind,
                                EventKind::Modify(ModifyKind::Any | ModifyKind::Data(_))) && path.is_dir();
                            if object_write || directory_write { continue; }
                            let admin = key.join("worktrees");
                            let linked_metadata = path == admin || path.starts_with(&admin);
                            if linked_metadata { batch.discoveries.insert(key.clone()); }
                            if !linked_metadata && !metadata.iter().any(|tree| tree.common == key) {
                                batch.repositories.insert(key.clone());
                            }
                            if repository_topology(&path, common, &event.kind) {
                                batch.discoveries.insert(key);
                                batch.topology = true;
                            }
                            if repository_watch_root(&path, common, &event.kind) {
                                batch.topology = true;
                                for (root, registration) in &self.roots {
                                    if registration.targets.iter().any(|target| matches!(target,
                                        Target::Repository(repository)
                                            if normalize(repository) == normalize(common))) {
                                        invalid_roots.insert(root.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for registration in self.roots.values() {
                for target in &registration.targets {
                    match target {
                        Target::Config(config) => {
                            if config_event(&path, config, &event.kind) {
                                batch.config = true;
                                if normalize(config).starts_with(&path) && topology_event(&event.kind) {
                                    batch.topology = true;
                                }
                            }
                        }
                        Target::GlobalExclude(exclude) if global_exclude_event(&path, exclude, &event.kind) => {
                            batch.global_excludes = true;
                            let keys = self.worktree_keys();
                            self.plan_dirty.extend(keys.iter().cloned());
                            batch.plans.extend(keys);
                            if normalize(exclude).starts_with(&path) && topology_event(&event.kind) {
                                batch.topology = true;
                            }
                        }
                        Target::Launch(launch) if launch_event(&path, launch, &event.kind) => {
                            batch.launch = true;
                            batch.topology = true;
                        }
                        Target::Worktree(key) => {
                            let tree_path = &key.path;
                            if (path.starts_with(tree_path)
                                || tree_path.starts_with(&path) && topology_event(&event.kind))
                                && (path == *tree_path || tree_path.starts_with(&path)
                                    || !repositories.iter().any(|common| intersects(&path, common)))
                            {
                                let worktree_topology = topology_event(&event.kind);
                                let tree_path = normalize(tree_path);
                                let root_topology = worktree_topology
                                    && (path == tree_path || tree_path.starts_with(&path));
                                Self::mark_worktree(batch, key, &path, &event.kind, false);
                                if self.plan_event(&path, key, &event.kind) {
                                    self.plan_dirty.insert(key.clone());
                                    batch.plans.insert(key.clone());
                                }
                                if root_topology {
                                    batch.topology = true;
                                }
                            }
                        }
                        Target::WorktreeMetadata(key, directory) if metadata.contains(key) => {
                            Self::mark_worktree(batch, key, &path, &event.kind, true);
                            if metadata_plan_event(&path, directory, &key.common, &event.kind) {
                                self.plan_dirty.insert(key.clone());
                                batch.plans.insert(key.clone());
                            }
                        }
                        Target::Repository(_) | Target::Launch(_) | Target::GlobalExclude(_)
                            | Target::WorktreeMetadata(_, _) => {}
                    }
                }
            }
        }
        if !invalid_roots.is_empty() {
            for root in &invalid_roots {
                if let Some(registration) = self.roots.get(root) {
                    for target in &registration.targets {
                        if let Target::Worktree(key) = target {
                            Self::mark_worktree(batch, key, root, &EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::Both)), true);
                        }
                    }
                }
            }
            if let Some(watcher) = self.watcher.as_mut() {
                for root in &invalid_roots {
                    let _ = watcher.unwatch(root);
                }
            }
            for root in invalid_roots {
                self.roots.remove(&root);
            }
        }
    }

    fn mark_worktree(batch: &mut WatchBatch, key: &WorktreeKey, path: &Path,
        kind: &EventKind, force: bool) {
        batch.worktrees.insert(key.clone());
        batch.worktree_paths.entry(key.clone()).or_default()
            .insert(path.to_path_buf(), *kind);
        if force { batch.force_worktrees.insert(key.clone()); }
    }

    fn worktree_keys(&self) -> HashSet<WorktreeKey> {
        self.roots.values()
            .flat_map(|registration| registration.targets.iter()).filter_map(|target| {
            match target {
                Target::Worktree(key) | Target::WorktreeMetadata(key, _) => Some(key.clone()),
                _ => None,
            }
        }).collect()
    }

    fn plan_event(&self, path: &Path, key: &WorktreeKey, kind: &EventKind) -> bool {
        let tree = normalize(&key.path);
        if !path.starts_with(&tree) { return false; }
        if self.ignored_event(path, key, kind) { return false; }
        if path.file_name().is_some_and(|name| name == ".gitignore") { return true; }
        topology_event(kind) && (self.roots.contains_key(path) || path.is_dir()
            || matches!(kind, EventKind::Create(notify::event::CreateKind::Folder)
                | EventKind::Remove(notify::event::RemoveKind::Folder)))
    }

    fn ignored_event(&self, path: &Path, key: &WorktreeKey, kind: &EventKind) -> bool {
        let Some(plan) = self.plans.get(key) else { return false; };
        if !plan.ignored_directories.iter().any(|directory| path == directory || path.starts_with(directory)) {
            return false;
        }
        if plan.tracked_files.contains(path) || topology_event(kind) && plan.tracked_directories.contains(path) {
            return false;
        }
        !rule_file(path)
    }
}

#[cfg(test)]
fn registrations(targets: &WatchTargets) -> HashMap<PathBuf, Registration> {
    let mut plans = HashMap::new();
    for (key, path) in &targets.worktrees {
        if let Ok(paths) = git::watch_paths(path, &Cancel::new()) {
            plans.insert(key.clone(), Arc::new(plan_for(path, &paths)));
        }
    }
    registrations_with_plans(targets, &plans)
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
        add_recovery(&mut result, &exclude, Target::GlobalExclude(exclude.clone()));
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

fn add_existing_or_parent(result: &mut HashMap<PathBuf, Registration>, path: &Path,
    mode: RecursiveMode, target: Target) {
    if path.is_dir() {
        add_target(result, path.to_path_buf(), mode, target);
    } else {
        add_recovery(result, path, target);
    }
}

fn add_recovery(result: &mut HashMap<PathBuf, Registration>, path: &Path, target: Target) {
    add_recovery_with_mode(result, path, target, RecursiveMode::NonRecursive);
}

fn add_recovery_with_mode(result: &mut HashMap<PathBuf, Registration>, path: &Path,
    target: Target, mode: RecursiveMode) {
    let parent = path.parent().unwrap_or(path);
    let root = nearest_existing(parent).unwrap_or_else(|| parent.to_path_buf());
    let mode = if root == parent { RecursiveMode::NonRecursive } else { mode };
    add_target(result, root, mode, target);
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

fn config_event(path: &Path, config: &Path, kind: &EventKind) -> bool {
    let path = normalize(&absolute(path));
    let config = normalize(&absolute(config));
    path == config || config.starts_with(&path) && topology_event(kind)
}

fn global_exclude_event(path: &Path, exclude: &Path, kind: &EventKind) -> bool {
    config_event(path, exclude, kind)
}

fn metadata_event(path: &Path, directory: &Path, common: &Path, kind: &EventKind) -> bool {
    let directory = normalize(directory);
    if path == directory || directory.starts_with(path) { return topology_event(kind); }
    let Ok(relative) = path.strip_prefix(&directory) else { return false; };
    if directory != normalize(common) { return true; }
    relative.components().next().is_some_and(|part| {
        matches!(part.as_os_str().to_string_lossy().to_ascii_lowercase().as_str(),
            "head" | "index" | "index.lock" | "orig_head" | "config.worktree")
            || relative == Path::new("logs/HEAD") || relative == Path::new("logs/head")
            || relative == Path::new("info/exclude")
    })
}

fn metadata_plan_event(path: &Path, directory: &Path, common: &Path, kind: &EventKind) -> bool {
    if !metadata_event(path, directory, common, kind) {
        return false;
    }
    let directory = normalize(directory);
    let common = normalize(common);
    let Ok(relative) = path.strip_prefix(&directory) else { return false; };
    let name = relative.components().next()
        .map(|part| part.as_os_str().to_string_lossy().to_ascii_lowercase());
    if directory != common {
        return name.is_some_and(|name| name == "index" || name == "index.lock" || name == "config.worktree");
    }
    name.is_some_and(|name| name == "index" || name == "index.lock" || name == "config.worktree")
        || relative == Path::new("info/exclude")
}

fn rule_file(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == ".gitignore" || name == ".gitattributes")
}

fn recovery_root(path: &Path) -> PathBuf {
    let path = absolute(path);
    let parent = path.parent().unwrap_or(&path);
    nearest_existing(parent).unwrap_or_else(|| parent.to_path_buf())
}

fn nearest_existing(path: &Path) -> Option<PathBuf> {
    let mut path = absolute(path);
    loop {
        if path.exists() { return Some(path); }
        if !path.pop() { return None; }
    }
}

fn launch_event(path: &Path, launch: &Path, kind: &EventKind) -> bool {
    let path = normalize(&absolute(path));
    let launch = normalize(&absolute(launch));
    if (path == launch || launch.starts_with(&path)) && topology_event(kind) {
        return true;
    }
    if !launch_git_event(kind) || path.file_name().is_none_or(|name| name != ".git") {
        return false;
    }
    path.parent().is_some_and(|parent| {
        let parent = normalize(&absolute(parent));
        launch == parent || launch.starts_with(&parent)
    })
}

fn launch_git_event(kind: &EventKind) -> bool {
    matches!(kind, EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_))
}

fn repository_topology(path: &Path, common: &Path, kind: &EventKind) -> bool {
    let path = normalize(&absolute(path));
    let common = normalize(&absolute(common));
    if path == common || common.starts_with(&path) { return topology_event(kind); }
    let admin = common.join("worktrees");
    (path == admin || path.starts_with(&admin)) && matches!(kind,
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_)))
}

fn repository_watch_root(path: &Path, common: &Path, kind: &EventKind) -> bool {
    if !topology_event(kind) { return false; }
    let path = normalize(&absolute(path));
    let common = normalize(&absolute(common));
    [common.clone(), common.join("refs"), common.join("worktrees"), common.join("info")]
        .iter().any(|root| path == normalize(root))
}

fn topology_event(kind: &EventKind) -> bool {
    matches!(kind, EventKind::Create(_) | EventKind::Remove(_)
        | EventKind::Modify(ModifyKind::Name(_)))
}

fn intersects(path: &Path, target: &Path) -> bool {
    let path = normalize(&absolute(path));
    let target = normalize(&absolute(target));
    path == target || path.starts_with(&target) || target.starts_with(&path)
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
mod tests {
    use super::*;
    use notify::event::{CreateKind, DataChange, RemoveKind, RenameMode};

    fn boundary_with(root: PathBuf, targets: Vec<Target>) -> WatchBoundary {
        let mut boundary = WatchBoundary::new(Cancel::new());
        boundary.roots.insert(normalize(&root), Registration { mode: RecursiveMode::Recursive, targets });
        boundary
    }

    #[test]
    fn config_sibling_events_are_ignored() {
        let root = PathBuf::from("watch-root");
        let config = absolute(Path::new("watch-root/config.json"));
        let mut boundary = boundary_with(root.clone(), vec![Target::Config(config.clone())]);
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Create(CreateKind::File))
            .add_path(absolute(Path::new("watch-root/config.order.json"))), &mut batch);
        assert!(!batch.config);
        boundary.classify(Event::new(EventKind::Create(CreateKind::File)).add_path(config), &mut batch);
        assert!(batch.config);
    }

    #[test]
    fn linked_worktree_root_replacement_marks_it_changed() {
        let root = absolute(Path::new("watch-root/linked"));
        let common = absolute(Path::new("watch-root/common.git"));
        let key = WorktreeKey::new(&common, &root);
        let mut boundary = boundary_with(root.clone(), vec![Target::Worktree(key.clone())]);
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Remove(RemoveKind::Folder)).add_path(root), &mut batch);
        assert!(batch.worktrees.contains(&key));
        assert!(batch.topology);
    }

    #[test]
    fn worktree_admin_create_requests_discovery_but_ref_edit_does_not() {
        let common = absolute(Path::new("watch-root/.git"));
        let key = WorktreeKey::new(&common, Path::new("watch-root"));
        let mut boundary = boundary_with(common.clone(), vec![Target::Repository(common.clone())]);
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Create(CreateKind::Folder))
            .add_path(common.join("worktrees/new")), &mut batch);
        assert_eq!(batch.discoveries.len(), 1);
        assert!(batch.repositories.is_empty());
        let mut boundary = boundary_with(common.clone(), vec![Target::Repository(common)]);
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Remove(RemoveKind::File))
            .add_path(absolute(Path::new("watch-root/.git/refs/heads/main"))), &mut batch);
        assert!(batch.discoveries.is_empty());
        assert!(batch.repositories.len() == 1);
        assert!(key.path.ends_with("watch-root"));
    }

    #[test]
    fn object_store_writes_do_not_invalidate_worktree_counts() {
        let tree = absolute(Path::new("watch-root"));
        let common = tree.join(".git");
        let key = WorktreeKey::new(&common, &tree);
        let mut boundary = boundary_with(tree, vec![
            Target::Repository(common.clone()), Target::Worktree(key),
        ]);
        for kind in [EventKind::Create(CreateKind::File),
            EventKind::Modify(ModifyKind::Any)] {
            let mut batch = WatchBatch::default();
            boundary.classify(Event::new(kind).add_path(common.join("objects/e6/empty")), &mut batch);
            assert!(batch.is_empty());
        }
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(common.join("refs/heads/main")), &mut batch);
        assert!(batch.repositories.contains(&normalize(&common)));
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Remove(RemoveKind::Folder))
            .add_path(common.join("objects")), &mut batch);
        assert!(batch.repositories.contains(&normalize(&common)));
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(common.join("objects/info/alternates")), &mut batch);
        assert!(batch.repositories.contains(&normalize(&common)));
    }

    #[test]
    fn directory_events_mark_the_worktree_changed() {
        let directory = tempfile::Builder::new().prefix("wtm-watch-ignore-").tempdir().unwrap();
        let root = directory.path();
        let cancel = Cancel::new();
        crate::process::checked(Some(root), &crate::process::args(&["init", "--quiet"]), &cancel).unwrap();
        std::fs::write(root.join(".gitignore"), b"ignored/\n").unwrap();
        std::fs::write(root.join("visible.txt"), b"visible\n").unwrap();
        std::fs::create_dir(root.join("ignored")).unwrap();
        let common = root.join(".git");
        let key = WorktreeKey::new(&common, root);
        let mut boundary = boundary_with(root.to_path_buf(), vec![
            Target::Repository(common), Target::Worktree(key.clone()),
        ]);
        for kind in [EventKind::Create(CreateKind::File), EventKind::Modify(ModifyKind::Any),
            EventKind::Remove(RemoveKind::File), EventKind::Modify(ModifyKind::Name(RenameMode::From))] {
            let mut batch = WatchBatch::default();
            boundary.classify(Event::new(kind).add_path(root.join("ignored/artifact.txt")), &mut batch);
            assert!(batch.worktrees.contains(&key));
        }
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(root.join("visible.txt")), &mut batch);
        assert!(batch.worktrees.contains(&key));
    }

    #[test]
    fn parent_directory_modifications_do_not_mark_child_worktrees() {
        let parent = absolute(Path::new("watch-parent"));
        let root = parent.join("repo");
        let common = root.join(".git");
        let key = WorktreeKey::new(&common, &root);
        let mut boundary = boundary_with(parent.clone(), vec![
            Target::Repository(common), Target::Worktree(key),
        ]);
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Modify(ModifyKind::Any)).add_path(parent), &mut batch);
        assert!(batch.is_empty());
    }

    #[test]
    fn index_events_only_mark_the_owning_worktree() {
        let common = absolute(Path::new("watch-root/.git"));
        let main = WorktreeKey::new(&common, Path::new("watch-root"));
        let linked = WorktreeKey::new(&common, Path::new("linked-root"));
        let admin = common.join("worktrees/linked");
        let mut boundary = boundary_with(common.clone(), vec![
            Target::Repository(common.clone()),
            Target::WorktreeMetadata(main.clone(), common.clone()),
            Target::WorktreeMetadata(linked.clone(), admin.clone()),
        ]);
        for (path, expected) in [(common.join("index"), &main), (admin.join("index"), &linked)] {
            let mut batch = WatchBatch::default();
            boundary.classify(Event::new(EventKind::Modify(ModifyKind::Any)).add_path(path), &mut batch);
            assert_eq!(batch.worktrees, HashSet::from([expected.clone()]));
            assert!(batch.repositories.is_empty());
        }
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
            .add_path(common.join("HEAD")), &mut batch);
        assert_eq!(batch.worktrees, HashSet::from([main.clone()]));
        assert!(batch.plans.is_empty());
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(common.join("refs/heads/main")), &mut batch);
        assert!(batch.repositories.contains(&normalize(&common)));
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Create(CreateKind::File))
            .add_path(common.join("packed-refs.lock")), &mut batch);
        boundary.classify(Event::new(EventKind::Remove(RemoveKind::File))
            .add_path(common.join("packed-refs.lock")), &mut batch);
        assert!(batch.is_empty());
    }

    #[test]
    fn metadata_events_survive_ignored_noise_in_the_same_batch() {
        let root = absolute(Path::new("watch-root"));
        let common = root.join(".git");
        let key = WorktreeKey::new(&common, &root);
        let ignored = normalize(&root.join("ignored"));
        let plan = Arc::new(WorktreeWatchPlan {
            recursive_roots: vec![normalize(&root)], directory_gates: Vec::new(),
            ignored_directories: HashSet::from([ignored]),
            tracked_files: HashSet::new(), tracked_directories: HashSet::new(),
        });
        let mut boundary = boundary_with(root.clone(), vec![
            Target::Worktree(key.clone()), Target::WorktreeMetadata(key.clone(), common.clone()),
        ]);
        boundary.plans.insert(key.clone(), plan);
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Create(CreateKind::File))
            .add_path(root.join("ignored/noise.txt")), &mut batch);
        boundary.classify(Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(common.join("index")), &mut batch);
        boundary.filter_ignored_batch(&mut batch);
        assert_eq!(batch.worktrees, HashSet::from([key]));
    }

    #[test]
    fn launch_events_only_follow_git_markers_or_directory_topology() {
        let launch = absolute(Path::new("watch-root/nested/deeper"));
        assert!(launch_event(&launch.join(".git"), &launch,
            &EventKind::Create(CreateKind::Folder)));
        assert!(launch_event(&absolute(Path::new("watch-root/.git")), &launch,
            &EventKind::Modify(ModifyKind::Data(DataChange::Content))));
        assert!(launch_event(&absolute(Path::new("watch-root/nested")), &launch,
            &EventKind::Remove(RemoveKind::Folder)));
        assert!(!launch_event(&launch.join("ordinary.txt"), &launch,
            &EventKind::Create(CreateKind::File)));
        assert!(!launch_event(&launch.join(".gitignore"), &launch,
            &EventKind::Modify(ModifyKind::Data(DataChange::Content))));
        assert!(!launch_event(&launch.join("child/.git"), &launch,
            &EventKind::Create(CreateKind::Folder)));
    }

    #[test]
    fn repository_metadata_root_replacement_reattaches_without_discovery() {
        let common = absolute(Path::new("watch-root/.git"));
        let refs = common.join("refs");
        let mut boundary = boundary_with(common.clone(), vec![Target::Repository(common.clone())]);
        boundary.roots.insert(normalize(&refs), Registration {
            mode: RecursiveMode::Recursive,
            targets: vec![Target::Repository(common.clone())],
        });
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
            .add_path(refs.clone()), &mut batch);
        assert!(batch.topology);
        assert!(batch.discoveries.is_empty());
        assert!(batch.repositories.contains(&normalize(&common)));
        assert!(!boundary.roots.contains_key(&normalize(&refs)));

        let mut boundary = boundary_with(common.clone(), vec![Target::Repository(common.clone())]);
        boundary.roots.insert(normalize(&refs), Registration {
            mode: RecursiveMode::Recursive,
            targets: vec![Target::Repository(common.clone())],
        });
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
            .add_path(refs.join("heads/main")), &mut batch);
        assert!(!batch.topology);
        assert!(batch.discoveries.is_empty());
        assert!(boundary.roots.contains_key(&normalize(&refs)));
    }

    #[test]
    fn watcher_errors_request_monitor_repair() {
        let mut boundary = WatchBoundary::new(Cancel::new());
        boundary.sender.send(Err(notify::Error::generic("watch failed"))).unwrap();
        let now = Instant::now();
        let batch = boundary.drain(now);
        assert!(batch.full);
        assert_eq!(boundary.next_retry(), Some(now));
        assert!(boundary.degraded);
    }

    #[test]
    fn full_recovery_reloads_ignore_rules_changed_while_disconnected() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("repo");
        std::fs::create_dir_all(root.join("ignored/nested")).unwrap();
        let cancel = Cancel::new();
        crate::process::checked(Some(&root), &crate::process::args(&["init", "--quiet"]), &cancel).unwrap();
        std::fs::write(root.join(".gitignore"), b"ignored/\n").unwrap();
        let key = WorktreeKey::new(&root.join(".git"), &root);
        let targets = WatchTargets {
            config: directory.path().join("config.json"), launch: None,
            repositories: Vec::new(), worktrees: vec![(key.clone(), root.clone())],
            global_excludes: Vec::new(),
        };
        let mut boundary = WatchBoundary::new(cancel);
        boundary.reconcile(&targets, Instant::now());
        let changed = root.join("ignored/nested/visible.txt");
        let kind = EventKind::Modify(ModifyKind::Data(DataChange::Content));
        assert!(!boundary.event_policy.read().unwrap().allows(&changed, &kind));
        boundary.watcher = None;
        std::fs::write(root.join(".gitignore"), b"").unwrap();
        boundary.sender.send(Err(notify::Error::generic("lost events"))).unwrap();
        assert!(boundary.drain(Instant::now()).full);
        assert!(!boundary.reconcile(&targets, Instant::now()).degraded);
        let policy = boundary.event_policy.read().unwrap().clone();
        assert!(policy.allows(&changed, &kind));
        std::fs::write(&changed, b"visible after recovery\n").unwrap();
        forward_event(Ok(Event::new(kind).add_path(changed)), &boundary.sender, &boundary.cancel, &policy);
        assert!(boundary.drain(Instant::now()).worktrees.contains(&key));
    }

    #[test]
    fn replacing_native_watch_mode_requests_full_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let root = normalize(directory.path());
        let config = directory.path().join("config.json");
        let mut boundary = WatchBoundary::new(Cancel::new());
        boundary.watcher.as_mut().unwrap().watch(&root, RecursiveMode::Recursive).unwrap();
        boundary.roots.insert(root.clone(), Registration {
            mode: RecursiveMode::Recursive, targets: vec![Target::Config(config.clone())],
        });
        let status = boundary.reconcile(&WatchTargets {
            config, launch: None, repositories: Vec::new(), worktrees: Vec::new(),
            global_excludes: Vec::new(),
        }, Instant::now());
        assert!(status.full);
        assert!(!status.degraded);
        assert_eq!(boundary.roots[&root].mode, RecursiveMode::NonRecursive);
    }

    #[test]
    fn forced_rebuild_reports_full_reconcile() {
        let mut boundary = WatchBoundary::new(Cancel::new());
        boundary.force_rebuild = true;
        let now = Instant::now();
        let status = boundary.reconcile(&WatchTargets {
            config: PathBuf::from("config.json"),
            launch: None,
            repositories: Vec::new(),
            worktrees: Vec::new(),
            global_excludes: Vec::new(),
        }, now);
        assert!(status.full);
        assert!(!status.degraded);
        assert_eq!(boundary.next_retry(), None);
    }

    #[test]
    fn drain_limits_events_and_wakes_for_queued_remainder() {
        let cancel = Cancel::new();
        let mut boundary = WatchBoundary::new(cancel.clone());
        for index in 0..=MAX_DRAIN_EVENTS {
            let event = Event::new(EventKind::Create(CreateKind::File))
                .add_path(PathBuf::from(format!("watch-root/{index}")));
            boundary.sender.send(Ok(event)).unwrap();
        }
        let generation = cancel.generation();
        let batch = boundary.drain(Instant::now());
        assert!(batch.is_empty());
        assert_eq!(cancel.generation(), generation + 1);
    }

    #[cfg(windows)]
    #[test]
    fn windows_callback_filters_ignored_events_before_waking_or_queuing() {
        let root = absolute(Path::new("watch-root"));
        let ignored = root.join("ignored");
        let tracked = ignored.join("tracked.txt");
        let plan = Arc::new(WorktreeWatchPlan {
            recursive_roots: vec![root.clone()], directory_gates: Vec::new(),
            ignored_directories: HashSet::from([normalize(&ignored)]),
            tracked_files: HashSet::from([normalize(&tracked)]),
            tracked_directories: HashSet::from([normalize(&ignored)]),
        });
        let key = WorktreeKey::new(&root.join(".git"), &root);
        let mut boundary = WatchBoundary::new(Cancel::new());
        boundary.plans.insert(key.clone(), plan);
        boundary.publish_policy(&WatchTargets {
            config: ignored.join("config.json"), launch: Some(root.clone()),
            repositories: vec![root.join(".git")], worktrees: vec![(key, root.clone())],
            global_excludes: Vec::new(),
        });
        let policy = boundary.event_policy.read().unwrap().clone();
        let cancel = Cancel::new();
        let (sender, receiver) = mpsc::channel();
        let generation = cancel.generation();
        for path in [ignored.join("noise.txt"), ignored.join("noise/.gitignore")] {
            forward_event(Ok(Event::new(EventKind::Create(CreateKind::File))
                .add_path(path)), &sender, &cancel, &policy);
        }
        assert_eq!(cancel.generation(), generation);
        assert!(receiver.try_recv().is_err());

        forward_event(Ok(Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
            .add_path(tracked.clone())), &sender, &cancel, &policy);
        assert_eq!(cancel.generation(), generation + 1);
        assert_eq!(receiver.try_recv().unwrap().unwrap().paths, vec![tracked]);

        for path in [ignored.join("config.json"), ignored.join(".gitattributes"), root.join(".git/HEAD")] {
            forward_event(Ok(Event::new(EventKind::Modify(ModifyKind::Any))
                .add_path(path.clone())), &sender, &cancel, &policy);
            assert_eq!(receiver.try_recv().unwrap().unwrap().paths, vec![path]);
        }

        let visible = root.join("visible.txt");
        forward_event(Ok(Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
            .add_path(ignored.join("noise.txt")).add_path(visible.clone())),
            &sender, &cancel, &policy);
        let event = receiver.try_recv().unwrap().unwrap();
        assert_eq!(event.paths, vec![visible]);
    }

    #[test]
    fn failed_initial_plan_keeps_recursive_fallback_coverage() {
        let directory = tempfile::tempdir().unwrap();
        let tree = directory.path().join("tree");
        std::fs::create_dir(&tree).unwrap();
        let common = tree.join(".git");
        let key = WorktreeKey::new(&common, &tree);
        let mut boundary = WatchBoundary::new(Cancel::new());
        let status = boundary.reconcile(&WatchTargets {
            config: directory.path().join("config.json"), launch: None,
            repositories: Vec::new(), worktrees: vec![(key.clone(), tree.clone())],
            global_excludes: Vec::new(),
        }, Instant::now());
        assert!(!status.degraded);
        assert_eq!(boundary.roots[&normalize(&tree)].mode, RecursiveMode::Recursive);
        boundary.sender.send(Ok(Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(tree.join("edited.txt")))).unwrap();
        let batch = boundary.drain(Instant::now());
        assert!(batch.worktrees.contains(&key));
    }

    #[test]
    fn overlapping_worktree_policies_keep_changes_needed_by_either_repository() {
        let root = absolute(Path::new("watch-parent"));
        let child = root.join("child");
        let ignored = child.join("ignored");
        let changed = ignored.join("source.rs");
        let mut child_plan = fallback_plan(&child);
        child_plan.ignored_directories.insert(normalize(&ignored));
        let child_policy = WorktreeEventPolicy {
            key: WorktreeKey::new(&child.join(".git"), &child), root: normalize(&child),
            plan: Arc::new(child_plan), protected: Vec::new(),
        };
        let mut policy = EventPolicy { worktrees: vec![child_policy] };
        assert!(policy.filter(Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(changed.clone())).is_none());
        policy.worktrees.push(WorktreeEventPolicy {
            key: WorktreeKey::new(&root.join(".git"), &root), root: normalize(&root),
            plan: Arc::new(fallback_plan(&root)), protected: Vec::new(),
        });
        assert_eq!(policy.filter(Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(changed.clone())).unwrap().paths, vec![changed]);
    }

    #[test]
    fn later_plan_failure_does_not_keep_stale_ignore_rules() {
        let directory = tempfile::tempdir().unwrap();
        let tree = directory.path().join("tree");
        let ignored = tree.join("ignored");
        std::fs::create_dir_all(&ignored).unwrap();
        let changed = ignored.join("source.rs");
        let key = WorktreeKey::new(&tree.join(".git"), &tree);
        let mut plan = fallback_plan(&tree);
        plan.ignored_directories.insert(normalize(&ignored));
        let mut boundary = WatchBoundary::new(Cancel::new());
        boundary.plans.insert(key.clone(), Arc::new(plan));
        assert!(boundary.ignored_event(&normalize(&changed), &key, &EventKind::Modify(ModifyKind::Any)));
        boundary.refresh_plan(&key, &tree);
        assert!(boundary.plan_errors.contains(&key));
        boundary.publish_policy(&WatchTargets {
            config: directory.path().join("config.json"), launch: Some(tree.clone()),
            repositories: Vec::new(), worktrees: vec![(key, tree)], global_excludes: Vec::new(),
        });
        let policy = boundary.event_policy.read().unwrap().clone();
        assert_eq!(policy.filter(Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(changed.clone())).unwrap().paths, vec![changed]);
    }

    #[test]
    fn deferred_plans_keep_nested_startup_events_without_git_planning() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("repo");
        std::fs::create_dir_all(root.join("nested/deep")).unwrap();
        let key = WorktreeKey::new(&root.join(".git"), &root);
        let targets = WatchTargets {
            config: directory.path().join("config.json"), launch: None,
            repositories: Vec::new(), worktrees: vec![(key.clone(), root.clone())],
            global_excludes: Vec::new(),
        };
        let mut boundary = WatchBoundary::new(Cancel::new());
        boundary.before_plan = Some(Box::new(|| panic!("detailed planning ran before initial counts")));
        assert!(!boundary.prepare(&targets, Instant::now()).degraded);
        let changed = root.join("nested/deep/startup.txt");
        std::fs::write(&changed, b"startup\n").unwrap();
        boundary.sender.send(Ok(Event::new(EventKind::Create(CreateKind::File))
            .add_path(changed))).unwrap();
        let batch = boundary.drain(Instant::now());
        assert!(batch.worktrees.contains(&key));
        assert!(boundary.plan_dirty.contains(&key));
    }

    #[test]
    fn ignored_directories_use_gates_and_never_recursive_native_roots() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        let cancel = Cancel::new();
        crate::process::checked(Some(&repo), &crate::process::args(&["init", "--quiet"]), &cancel).unwrap();
        std::fs::write(repo.join(".gitignore"), b"ignored/\n").unwrap();
        std::fs::create_dir_all(repo.join("ignored/nested")).unwrap();
        std::fs::create_dir_all(repo.join("allowed/nested/deep")).unwrap();
        std::fs::write(repo.join("ignored/nested/tracked.txt"), b"tracked\n").unwrap();
        crate::process::checked(Some(&repo), &crate::process::args(&["add", ".gitignore"]), &cancel).unwrap();
        crate::process::checked(Some(&repo), &crate::process::args(&["add", "-f", "ignored/nested/tracked.txt"]), &cancel).unwrap();
        let common = repo.join(".git");
        let key = WorktreeKey::new(&common, &repo);
        let roots = registrations(&WatchTargets {
            config: directory.path().join("config.json"),
            launch: None,
            repositories: vec![common],
            worktrees: vec![(key, repo.clone())],
            global_excludes: Vec::new(),
        });
        #[cfg(not(windows))]
        let ignored = normalize(&repo.join("ignored"));
        #[cfg(not(windows))]
        assert_eq!(roots[&ignored].mode, RecursiveMode::NonRecursive);
        #[cfg(not(windows))]
        assert_eq!(roots[&normalize(&repo.join("ignored/nested"))].mode, RecursiveMode::NonRecursive);
        #[cfg(not(windows))]
        assert_eq!(roots[&normalize(&repo.join("allowed"))].mode, RecursiveMode::Recursive);
        #[cfg(not(windows))]
        assert!(!roots.contains_key(&normalize(&repo.join("allowed/nested"))));
        #[cfg(not(windows))]
        assert!(roots[&normalize(&repo)].targets.iter().all(|target| matches!(target, Target::Worktree(_))));
        #[cfg(not(windows))]
        assert!(roots.iter().filter(|(_, registration)| registration.mode == RecursiveMode::Recursive)
            .all(|(path, _)| !path.starts_with(&ignored)));
        #[cfg(windows)]
        assert_eq!(roots[&normalize(&repo)].mode, RecursiveMode::Recursive);
        #[cfg(windows)]
        assert!(roots[&normalize(&repo)].targets.iter().any(|target| matches!(target, Target::Repository(_))));
        #[cfg(windows)]
        assert!(roots.keys().all(|path| *path == normalize(&repo) || !path.starts_with(&repo)));
        #[cfg(not(windows))]
        let recursive: Vec<_> = roots.iter().filter_map(|(path, registration)|
            (registration.mode == RecursiveMode::Recursive).then_some(path)).collect();
        #[cfg(not(windows))]
        assert!(recursive.iter().enumerate().all(|(index, path)| recursive.iter().enumerate()
            .all(|(other, other_path)| index == other || !path.starts_with(*other_path))));
    }

    #[test]
    fn existing_repository_and_worktree_use_narrow_roots() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().join("repo");
        let common = repo.join(".git");
        std::fs::create_dir(&repo).unwrap();
        crate::process::checked(Some(&repo), &crate::process::args(&["init", "--quiet"]), &Cancel::new()).unwrap();
        for name in ["refs", "worktrees", "info"] {
            std::fs::create_dir_all(common.join(name)).unwrap();
        }
        let key = WorktreeKey::new(&common, &repo);
        let roots = registrations(&WatchTargets {
            config: directory.path().join("config.json"),
            launch: None,
            repositories: vec![common.clone()],
            worktrees: vec![(key, repo.clone())],
            global_excludes: vec![common.join("config")],
        });
        assert_eq!(roots[&normalize(directory.path())].mode, RecursiveMode::NonRecursive);
        #[cfg(windows)]
        {
            assert_eq!(roots[&normalize(&repo)].mode, RecursiveMode::Recursive);
            assert!(!roots.contains_key(&normalize(&common)));
            for name in ["refs", "worktrees", "info"] {
                assert!(!roots.contains_key(&normalize(&common.join(name))));
            }
            assert!(roots[&normalize(&repo)].targets.iter()
                .any(|target| matches!(target, Target::Repository(_))));
        }
        #[cfg(not(windows))]
        {
            assert_eq!(roots[&normalize(&repo)].mode, RecursiveMode::NonRecursive);
            assert_eq!(roots[&normalize(&common)].mode, RecursiveMode::NonRecursive);
            for name in ["refs", "worktrees", "info"] {
                assert_eq!(roots[&normalize(&common.join(name))].mode, RecursiveMode::Recursive);
            }
        }
    }

    #[test]
    fn existing_launch_uses_an_exact_nonrecursive_root() {
        let directory = tempfile::tempdir().unwrap();
        let launch = directory.path().join("project/nested");
        std::fs::create_dir_all(&launch).unwrap();
        let roots = registrations(&WatchTargets {
            config: directory.path().join("config.json"),
            launch: Some(launch.clone()),
            repositories: Vec::new(),
            worktrees: Vec::new(),
            global_excludes: Vec::new(),
        });
        let registration = &roots[&normalize(&launch)];
        assert_eq!(registration.mode, RecursiveMode::NonRecursive);
        assert!(registration.targets.iter().any(|target| {
            matches!(target, Target::Launch(path) if normalize(path) == normalize(&launch))
        }));
    }

    #[cfg(windows)]
    #[test]
    fn windows_merges_nested_registrations_onto_the_worktree_root() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().join("repo");
        let common = repo.join(".git");
        std::fs::create_dir(&repo).unwrap();
        crate::process::checked(Some(&repo), &crate::process::args(&["init", "--quiet"]), &Cancel::new()).unwrap();
        for name in ["refs", "worktrees", "info"] {
            std::fs::create_dir_all(common.join(name)).unwrap();
        }
        let key = WorktreeKey::new(&common, &repo);
        let mut boundary = WatchBoundary::new(Cancel::new());
        let status = boundary.reconcile(&WatchTargets {
            config: directory.path().join("config.json"),
            launch: None,
            repositories: vec![common.clone()],
            worktrees: vec![(key, repo.clone())],
            global_excludes: Vec::new(),
        }, Instant::now());
        assert!(!status.degraded);
        assert_eq!(boundary.roots[&normalize(&repo)].mode, RecursiveMode::Recursive);
        assert!(!boundary.roots.contains_key(&normalize(&common)));
        for name in ["refs", "worktrees", "info"] {
            assert!(!boundary.roots.contains_key(&normalize(&common.join(name))));
        }
        assert!(boundary.roots[&normalize(&repo)].targets.iter().any(|target| matches!(target, Target::Repository(_))));
    }

    #[cfg(windows)]
    #[test]
    fn windows_exact_worktree_registration_allows_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().join("repo");
        let common = repo.join(".git");
        std::fs::create_dir(&repo).unwrap();
        crate::process::checked(Some(&repo), &crate::process::args(&["init", "--quiet"]), &Cancel::new()).unwrap();
        for name in ["refs", "worktrees", "info"] {
            std::fs::create_dir_all(common.join(name)).unwrap();
        }
        let key = WorktreeKey::new(&common, &repo);
        let mut boundary = WatchBoundary::new(Cancel::new());
        let status = boundary.reconcile(&WatchTargets {
            config: directory.path().join("config.json"),
            launch: None,
            repositories: vec![common],
            worktrees: vec![(key, repo.clone())],
            global_excludes: Vec::new(),
        }, Instant::now());
        assert!(!status.degraded);
        boundary.watcher = None;
        let moved = directory.path().join("moved");
        std::fs::rename(&repo, &moved).unwrap();
        std::fs::rename(moved, repo).unwrap();
    }
}
