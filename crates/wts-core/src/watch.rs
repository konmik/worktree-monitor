use crate::poll::{normalize, WorktreeKey};
use crate::Cancel;
use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};

const EVENT_QUEUE_CAPACITY: usize = 256;
const MAX_DRAIN_EVENTS: usize = 128;
pub(crate) const WATCH_RETRY: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum Target {
    Config(PathBuf),
    Launch(PathBuf),
    Repository(PathBuf),
    Worktree(WorktreeKey),
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
}

impl WatchBatch {
    pub(crate) fn is_empty(&self) -> bool {
        !self.config && !self.launch && self.repositories.is_empty() && self.discoveries.is_empty()
            && self.worktrees.is_empty() && !self.topology && !self.full
    }
}

pub(crate) struct WatchTargets {
    pub(crate) config: PathBuf,
    pub(crate) launch: Option<PathBuf>,
    pub(crate) repositories: Vec<PathBuf>,
    pub(crate) worktrees: Vec<(WorktreeKey, PathBuf)>,
}

pub(crate) struct WatchStatus {
    pub(crate) full: bool,
    pub(crate) degraded: bool,
}

pub(crate) struct WatchBoundary {
    watcher: Option<RecommendedWatcher>,
    sender: SyncSender<Result<Event, notify::Error>>,
    receiver: Receiver<Result<Event, notify::Error>>,
    overflow: Arc<AtomicBool>,
    cancel: Cancel,
    roots: HashMap<PathBuf, Registration>,
    retry_due: Option<Instant>,
    force_rebuild: bool,
    degraded: bool,
    #[cfg(test)]
    fail_reconcile: bool,
}

impl WatchBoundary {
    pub(crate) fn new(cancel: Cancel) -> Self {
        let (sender, receiver) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let overflow = Arc::new(AtomicBool::new(false));
        let mut boundary = Self {
            watcher: None,
            sender,
            receiver,
            overflow,
            cancel,
            roots: HashMap::new(),
            retry_due: None,
            force_rebuild: false,
            degraded: false,
            #[cfg(test)]
            fail_reconcile: false,
        };
        let now = Instant::now();
        boundary.create_watcher(now);
        boundary
    }

    pub(crate) fn reconcile(&mut self, targets: &WatchTargets, now: Instant) -> WatchStatus {
        let desired = registrations(targets);
        #[cfg(test)]
        if self.fail_reconcile {
            self.force_rebuild = true;
            self.degraded = true;
            self.retry_due = Some(now + WATCH_RETRY);
            return WatchStatus { full: true, degraded: true };
        }
        let changed = self.force_rebuild || self.watcher.is_none() || self.roots != desired;
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
            let stale: Vec<_> = self.roots.iter()
                .filter(|(path, registration)| desired.get(*path) != Some(registration))
                .map(|(path, _)| path.clone())
                .collect();
            for path in stale {
                if watcher.unwatch(&path).is_err() {
                    failed = true;
                }
                self.roots.remove(&path);
            }
            let additions: Vec<_> = desired.iter()
                .filter(|(path, registration)| self.roots.get(*path) != Some(registration))
                .map(|(path, registration)| (path.clone(), registration.clone()))
                .collect();
            for (path, registration) in additions {
                match watcher.watch(&path, registration.mode) {
                    Ok(()) => { self.roots.insert(path, registration); }
                    Err(_) => { failed = true; full = true; }
                }
            }
        }
        if failed { full = true; }
        self.force_rebuild = failed;
        self.degraded = failed || self.watcher.is_none() || self.roots != desired;
        if self.degraded {
            self.retry_due = Some(now + WATCH_RETRY);
        } else {
            self.retry_due = None;
            self.force_rebuild = false;
        }
        WatchStatus { full, degraded: self.degraded }
    }

    pub(crate) fn drain(&mut self, now: Instant) -> WatchBatch {
        let mut batch = WatchBatch::default();
        if self.overflow.swap(false, Ordering::AcqRel) {
            batch.full = true;
            self.force_rebuild = true;
            self.degraded = true;
        }
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
        if drained == MAX_DRAIN_EVENTS { self.cancel.wake(); }
        if batch.full {
            self.force_rebuild = true;
            self.degraded = true;
            self.retry_due = Some(now);
        }
        batch
    }

    pub(crate) fn next_retry(&self) -> Option<Instant> { self.retry_due }

    pub(crate) fn degraded(&self) -> bool { self.degraded }

    #[cfg(test)]
    pub(crate) fn fail_reconcile_for_test(&mut self) { self.fail_reconcile = true; }

    fn create_watcher(&mut self, now: Instant) {
        let sender = self.sender.clone();
        let overflow = self.overflow.clone();
        let cancel = self.cancel.clone();
        let watcher = RecommendedWatcher::new(move |result| {
            match sender.try_send(result) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => { overflow.store(true, Ordering::Release); }
                Err(TrySendError::Disconnected(_)) => {}
            }
            cancel.wake();
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
            return;
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
            if topology_event(&event.kind) && self.roots.contains_key(&path) {
                invalid_roots.insert(path.clone());
                batch.topology = true;
            }
            let mut repositories = HashSet::new();
            for registration in self.roots.values() {
                for target in &registration.targets {
                    if let Target::Repository(common) = target {
                        if intersects(&path, common) {
                            let key = normalize(common);
                            repositories.insert(key.clone());
                            batch.repositories.insert(key.clone());
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
                            let path = normalize(&path);
                            let config = normalize(config);
                            if path == config {
                                batch.config = true;
                            } else if config.starts_with(&path) && topology_event(&event.kind) {
                                batch.config = true;
                                batch.topology = true;
                            }
                        }
                        Target::Launch(launch) if launch_event(&path, launch, &event.kind) => {
                            batch.launch = true;
                            batch.topology = true;
                        }
                        Target::Worktree(key) => {
                            let tree_path = &key.path;
                            if intersects(&path, tree_path)
                                && !repositories.iter().any(|common| intersects(&path, common))
                            {
                                batch.worktrees.insert(key.clone());
                                let path = normalize(&path);
                                let tree_path = normalize(tree_path);
                                if (path == tree_path || tree_path.starts_with(&path))
                                    && topology_event(&event.kind) {
                                    batch.topology = true;
                                }
                            }
                        }
                        Target::Repository(_) | Target::Launch(_) => {}
                    }
                }
            }
        }
        if !invalid_roots.is_empty() {
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
}

fn registrations(targets: &WatchTargets) -> HashMap<PathBuf, Registration> {
    let mut result = HashMap::new();
    let worktree_paths: Vec<_> = targets.worktrees.iter()
        .map(|(key, path)| (key.common.clone(), absolute(path)))
        .filter(|(_, path)| path.is_dir()).collect();
    let (config_root, config_mode) = recovery_root(&targets.config);
    add_target(&mut result, config_root, config_mode, Target::Config(absolute(&targets.config)));
    add_ancestor(&mut result, &targets.config, Target::Config(absolute(&targets.config)));
    if let Some(launch) = &targets.launch {
        let launch = absolute(launch);
        let target = Target::Launch(launch.clone());
        let (launch_root, launch_mode) = recovery_root(&launch);
        add_target(&mut result, launch_root, launch_mode, target.clone());
        if launch.is_dir() {
            add_target(&mut result, launch.clone(), RecursiveMode::NonRecursive, target.clone());
        }
        add_ancestor(&mut result, &launch, target);
    }
    for common in &targets.repositories {
        let common = absolute(common);
        let nested_in_worktree = cfg!(windows) && worktree_paths.iter().any(|(worktree_common, path)| {
            normalize(worktree_common) == normalize(&common)
                && normalize(&common).starts_with(normalize(path))
        });
        if !nested_in_worktree {
            add_existing_or_parent(&mut result, &common, RecursiveMode::NonRecursive,
                Target::Repository(common.clone()));
            add_ancestor(&mut result, &common, Target::Repository(common.clone()));
            for directory in [common.join("refs"), common.join("worktrees"), common.join("info")] {
                if directory.is_dir() {
                    add_target(&mut result, directory.clone(), RecursiveMode::Recursive,
                        Target::Repository(common.clone()));
                }
            }
        }
    }
    for (key, path) in &targets.worktrees {
        let path = absolute(path);
        let (parent, parent_mode) = recovery_root(&path);
        add_target(&mut result, parent, parent_mode, Target::Worktree(key.clone()));
        add_ancestor(&mut result, &path, Target::Worktree(key.clone()));
        if path.is_dir() {
            add_target(&mut result, path.clone(), RecursiveMode::Recursive, Target::Worktree(key.clone()));
            if cfg!(windows) {
                for common in &targets.repositories {
                    let common = absolute(common);
                    if normalize(&common) == normalize(key.common.as_path())
                        && normalize(&common).starts_with(normalize(&path)) {
                        add_target(&mut result, absolute(&path), RecursiveMode::Recursive,
                            Target::Repository(common));
                    }
                }
            }
        }
    }
    for registration in result.values_mut() {
        registration.targets.sort();
        registration.targets.dedup();
    }
    result
}

fn add_existing_or_parent(result: &mut HashMap<PathBuf, Registration>, path: &Path,
    mode: RecursiveMode, target: Target) {
    let root = if path.exists() { path.to_path_buf() } else {
        nearest_existing(path).unwrap_or_else(|| path.to_path_buf())
    };
    let parent = path.parent().unwrap_or(path);
    let mode = if root != path && root != parent { RecursiveMode::Recursive } else { mode };
    add_target(result, root, mode, target);
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

fn add_ancestor(result: &mut HashMap<PathBuf, Registration>, path: &Path, target: Target) {
    let path = absolute(path);
    let parent = path.parent().unwrap_or(&path);
    if parent == path { return; }
    let (root, mode) = recovery_root(parent);
    add_target(result, root, mode, target);
}

fn recovery_root(path: &Path) -> (PathBuf, RecursiveMode) {
    let path = absolute(path);
    let parent = path.parent().unwrap_or(&path);
    let root = nearest_existing(parent).unwrap_or_else(|| parent.to_path_buf());
    let mode = if root == parent { RecursiveMode::NonRecursive } else { RecursiveMode::Recursive };
    (root, mode)
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
    fn worktree_admin_create_requests_discovery_but_ref_edit_does_not() {
        let common = absolute(Path::new("watch-root/.git"));
        let key = WorktreeKey::new(&common, Path::new("watch-root"));
        let mut boundary = boundary_with(common.clone(), vec![Target::Repository(common.clone())]);
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Create(CreateKind::Folder))
            .add_path(common.join("worktrees/new")), &mut batch);
        assert_eq!(batch.discoveries.len(), 1);
        assert!(batch.repositories.contains(&normalize(&common)));
        let mut boundary = boundary_with(common.clone(), vec![Target::Repository(common)]);
        let mut batch = WatchBatch::default();
        boundary.classify(Event::new(EventKind::Remove(RemoveKind::File))
            .add_path(absolute(Path::new("watch-root/.git/refs/heads/main"))), &mut batch);
        assert!(batch.discoveries.is_empty());
        assert!(batch.repositories.len() == 1);
        assert!(key.path.ends_with("watch-root"));
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
    fn overflow_requests_full_reconcile_and_retry() {
        let mut boundary = WatchBoundary::new(Cancel::new());
        boundary.overflow.store(true, Ordering::Release);
        let now = Instant::now();
        let batch = boundary.drain(now);
        assert!(batch.full);
        assert_eq!(boundary.next_retry(), Some(now));
        assert!(boundary.degraded());
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

    #[test]
    fn existing_repository_and_worktree_use_narrow_roots() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().join("repo");
        let common = repo.join(".git");
        for name in ["refs", "worktrees", "info"] {
            std::fs::create_dir_all(common.join(name)).unwrap();
        }
        let key = WorktreeKey::new(&common, &repo);
        let roots = registrations(&WatchTargets {
            config: directory.path().join("config.json"),
            launch: None,
            repositories: vec![common.clone()],
            worktrees: vec![(key, repo.clone())],
        });
        assert_eq!(roots[&normalize(&repo)].mode, RecursiveMode::Recursive);
        assert_eq!(roots[&normalize(directory.path())].mode, RecursiveMode::NonRecursive);
        #[cfg(not(windows))]
        assert_eq!(roots[&normalize(&common)].mode, RecursiveMode::NonRecursive);
        #[cfg(not(windows))]
        for name in ["refs", "worktrees", "info"] {
            assert_eq!(roots[&normalize(&common.join(name))].mode, RecursiveMode::Recursive);
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
        });
        let registration = &roots[&normalize(&launch)];
        assert_eq!(registration.mode, RecursiveMode::NonRecursive);
        assert!(registration.targets.iter().any(|target| {
            matches!(target, Target::Launch(path) if normalize(path) == normalize(&launch))
        }));
    }

    #[cfg(windows)]
    #[test]
    fn windows_accepts_narrow_nested_registrations() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().join("repo");
        let common = repo.join(".git");
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
        }, Instant::now());
        assert!(!status.degraded);
        assert_eq!(boundary.roots[&normalize(&repo)].mode, RecursiveMode::Recursive);
        assert!(!boundary.roots.contains_key(&normalize(&common)));
        for name in ["refs", "worktrees", "info"] {
            assert!(!boundary.roots.contains_key(&normalize(&common.join(name))));
        }
        assert!(boundary.roots[&normalize(&repo)].targets.iter().any(|target| {
            matches!(target, Target::Repository(path) if normalize(path) == normalize(&common))
        }));
    }

    #[cfg(windows)]
    #[test]
    fn windows_exact_worktree_registration_allows_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().join("repo");
        let common = repo.join(".git");
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
        }, Instant::now());
        assert!(!status.degraded);
        let moved = directory.path().join("moved");
        std::fs::rename(&repo, &moved).unwrap();
        std::fs::rename(moved, repo).unwrap();
    }
}
