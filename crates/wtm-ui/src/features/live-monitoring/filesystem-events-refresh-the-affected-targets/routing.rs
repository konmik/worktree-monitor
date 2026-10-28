use super::{absolute, normalize, WatchBoundary, WatchBatch, Target, WorktreeKey};
use notify::event::ModifyKind;
use notify::{Event, EventKind, Watcher};
use std::collections::HashSet;
use std::path::Path;

impl WatchBoundary {
    pub(super) fn classify(&mut self, event: Event, batch: &mut WatchBatch) {
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
            if matches!(event.kind, EventKind::Modify(ModifyKind::Any | ModifyKind::Data(_)))
                && std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink()) {
                continue;
            }
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
                            if !repository_event(&path, &key, &event.kind) { continue; }
                            let admin = key.join("worktrees");
                            let linked_metadata = path == admin || path.starts_with(&admin);
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
                        Target::RepositorySource(common, source) if global_exclude_event(&path, source, &event.kind) => {
                            let keys: HashSet<_> = self.worktree_keys().into_iter()
                                .filter(|key| key.common == normalize(common)).collect();
                            for key in &keys { Self::mark_worktree(batch, key, &path, &event.kind, true); }
                            self.plan_dirty.extend(keys.iter().cloned());
                            batch.plans.extend(keys);
                            batch.discoveries.insert(normalize(common));
                            if normalize(source).starts_with(&path) && topology_event(&event.kind) {
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
                                let directory_topology = worktree_topology
                                    && self.plans.get(key).is_some_and(|plan|
                                        plan.known_directories.contains(&path) || plan.tracked_directories.contains(&path))
                                    && !self.ignored_event(&path, key, &event.kind);
                                Self::mark_worktree(batch, key, &path, &event.kind, root_topology || directory_topology);
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
                        Target::Repository(_) | Target::Launch(_) | Target::GlobalExclude(_) | Target::RepositorySource(_, _)
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

    pub(super) fn mark_worktree(batch: &mut WatchBatch, key: &WorktreeKey, path: &Path,
        kind: &EventKind, force: bool) {
        batch.worktrees.insert(key.clone());
        batch.worktree_paths.entry(key.clone()).or_default()
            .insert(path.to_path_buf(), *kind);
        if force { batch.force_worktrees.insert(key.clone()); }
    }

    pub(super) fn worktree_keys(&self) -> HashSet<WorktreeKey> {
        self.roots.values()
            .flat_map(|registration| registration.targets.iter()).filter_map(|target| {
            match target {
                Target::Worktree(key) | Target::WorktreeMetadata(key, _) => Some(key.clone()),
                _ => None,
            }
        }).collect()
    }
}

pub(super) fn config_event(path: &Path, config: &Path, kind: &EventKind) -> bool {
    let path = normalize(&absolute(path));
    let config = normalize(&absolute(config));
    path == config || config.starts_with(&path) && topology_event(kind)
}

pub(super) fn global_exclude_event(path: &Path, exclude: &Path, kind: &EventKind) -> bool {
    config_event(path, exclude, kind)
}

fn repository_event(path: &Path, common: &Path, kind: &EventKind) -> bool {
    if path == common || common.starts_with(path) { return topology_event(kind); }
    if repository_topology(path, common, kind) { return true; }
    let Ok(relative) = path.strip_prefix(common) else { return false; };
    if relative.starts_with("refs") { return true; }
    if relative == Path::new("objects") { return topology_event(kind); }
    ["HEAD", "head", "index", "config", "config.worktree", "packed-refs", "shallow",
        "info/exclude", "info/attributes", "info/grafts", "objects/info/alternates"]
        .iter().any(|name| {
            let relevant = Path::new(name);
            relative == relevant || topology_event(kind) && relevant.starts_with(relative)
        })
}

pub(super) fn metadata_event(path: &Path, directory: &Path, common: &Path, kind: &EventKind) -> bool {
    let directory = normalize(directory);
    if path == directory || directory.starts_with(path) { return topology_event(kind); }
    let Ok(relative) = path.strip_prefix(&directory) else { return false; };
    let name = relative.to_string_lossy().to_ascii_lowercase();
    matches!(name.as_str(), "head" | "index" | "config.worktree")
        || directory == normalize(common) && name == "info/exclude"
}

pub(super) fn metadata_plan_event(path: &Path, directory: &Path, common: &Path, kind: &EventKind) -> bool {
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

pub(super) fn rule_file(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == ".gitignore" || name == ".gitattributes")
}

pub(super) fn launch_event(path: &Path, launch: &Path, kind: &EventKind) -> bool {
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

pub(super) fn launch_git_event(kind: &EventKind) -> bool {
    matches!(kind, EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_))
}

pub(super) fn repository_topology(path: &Path, common: &Path, kind: &EventKind) -> bool {
    let path = normalize(&absolute(path));
    let common = normalize(&absolute(common));
    if path == common || common.starts_with(&path) { return topology_event(kind); }
    let admin = common.join("worktrees");
    if path == admin { return topology_event(kind); }
    let Ok(relative) = path.strip_prefix(&admin) else { return false; };
    let parts: Vec<_> = relative.components().collect();
    parts.len() == 1 && topology_event(kind)
        || parts.len() == 2 && matches!(parts[1].as_os_str().to_string_lossy().as_ref(), "gitdir" | "commondir")
}

pub(super) fn repository_watch_root(path: &Path, common: &Path, kind: &EventKind) -> bool {
    if !topology_event(kind) { return false; }
    let path = normalize(&absolute(path));
    let common = normalize(&absolute(common));
    [common.clone(), common.join("refs"), common.join("worktrees"), common.join("info")]
        .iter().any(|root| path == normalize(root))
}

pub(super) fn topology_event(kind: &EventKind) -> bool {
    matches!(kind, EventKind::Create(_) | EventKind::Remove(_)
        | EventKind::Modify(ModifyKind::Name(_)))
}

pub(super) fn intersects(path: &Path, target: &Path) -> bool {
    let path = normalize(&absolute(path));
    let target = normalize(&absolute(target));
    path == target || path.starts_with(&target) || target.starts_with(&path)
}
