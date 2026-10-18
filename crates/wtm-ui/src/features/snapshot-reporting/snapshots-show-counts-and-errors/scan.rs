use crate::git::{discover, fallback_path, scan_tree};
use crate::{Cancel, Error, RepoEntry, Snapshot, WorktreeEntry};
use std::path::PathBuf;

pub(crate) fn scan(repos: &[PathBuf], config_error: Option<Error>, cancel: &Cancel) -> Snapshot {
    let repos = repos.iter().map(|common| {
        match discover(common, cancel) {
            Ok(discovered) => RepoEntry {
                common_dir: common.clone(),
                path: discovered.path,
                worktrees: Ok(discovered.trees.into_iter().map(|tree| WorktreeEntry {
                    path: tree.path.clone(),
                    result: tree.error.map_or_else(|| scan_tree(&tree.path, cancel), Err),
                }).collect()),
            },
            Err(error) => RepoEntry {
                common_dir: common.clone(),
                path: fallback_path(common),
                worktrees: Err(error),
            },
        }
    }).collect();
    Snapshot { repos, config_error }
}
