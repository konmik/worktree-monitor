use crate::poll::{normalize, WorktreeKey};
use crate::Cancel;
use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

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
}

impl WatchBatch {
    pub(crate) fn is_empty(&self) -> bool {
        !self.config && !self.launch && self.repositories.is_empty() && self.discoveries.is_empty()
            && self.worktrees.is_empty() && !self.topology && !self.full
            && !self.global_excludes
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

pub(crate) struct WatchBoundary {
    watcher: Option<RecommendedWatcher>,
    sender: Sender<Result<Event, notify::Error>>,
    receiver: Receiver<Result<Event, notify::Error>>,
    cancel: Cancel,
    roots: HashMap<PathBuf, Registration>,
    retry_due: Option<Instant>,
    force_rebuild: bool,
    degraded: bool,
}

impl WatchBoundary {
    pub(crate) fn new(cancel: Cancel) -> Self {
        let (sender, receiver) = mpsc::channel();
        let mut boundary = Self {
            watcher: None,
            sender,
            receiver,
            cancel,
            roots: HashMap::new(),
            retry_due: None,
            force_rebuild: false,
            degraded: false,
        };
        let now = Instant::now();
        boundary.create_watcher(now);
        boundary
    }

    pub(crate) fn reconcile(&mut self, targets: &WatchTargets, now: Instant) -> WatchStatus {
        let desired = registrations(targets);
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

    fn create_watcher(&mut self, now: Instant) {
        let sender = self.sender.clone();
        let cancel = self.cancel.clone();
        let watcher = RecommendedWatcher::new(move |result| {
            let _ = sender.send(result);
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
            let metadata: HashSet<_> = self.roots.values().flat_map(|registration| &registration.targets)
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
                                batch.worktrees.insert(key.clone());
                                if root_topology {
                                    batch.topology = true;
                                }
                            }
                        }
                        Target::WorktreeMetadata(key, _) if metadata.contains(key) => {
                            batch.worktrees.insert(key.clone());
                        }
                        Target::Repository(_) | Target::Launch(_) | Target::GlobalExclude(_)
                            | Target::WorktreeMetadata(_, _) => {}
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
    for exclude in &targets.global_excludes {
        let exclude = absolute(exclude);
        let target = Target::GlobalExclude(exclude.clone());
        if cfg!(windows) {
            if let Some((_, root)) = worktree_paths.iter()
                .filter(|(_, root)| normalize(&exclude).starts_with(normalize(root)))
                .max_by_key(|(_, root)| root.components().count()) {
                add_target(&mut result, root.clone(), RecursiveMode::Recursive, target);
                continue;
            }
        }
        let (root, mode) = recovery_root(&exclude);
        add_target(&mut result, root, mode, target.clone());
        add_ancestor(&mut result, &exclude, target);
    }
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
    })
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
        let directory = tempfile::Builder::new().prefix("wts-watch-ignore-").tempdir().unwrap();
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
            global_excludes: vec![common.join("config")],
        });
        assert_eq!(roots[&normalize(&repo)].mode, RecursiveMode::Recursive);
        assert_eq!(roots[&normalize(directory.path())].mode, RecursiveMode::NonRecursive);
        #[cfg(windows)]
        assert!(!roots.contains_key(&normalize(&common)));
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
            global_excludes: Vec::new(),
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
            global_excludes: Vec::new(),
        }, Instant::now());
        assert!(!status.degraded);
        let moved = directory.path().join("moved");
        std::fs::rename(&repo, &moved).unwrap();
        std::fs::rename(moved, repo).unwrap();
    }
}
