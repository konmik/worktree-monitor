use crate::process::{self, args};
use crate::{Cancel, Error, ErrorKind};
use std::path::{Path, PathBuf};

pub(crate) fn path(bytes: &[u8]) -> Result<PathBuf, Error> {
    if bytes.is_empty() { return Err(invalid("Git returned an empty path")); }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
    }
    #[cfg(not(unix))]
    {
        let text = std::str::from_utf8(bytes).map_err(|_| invalid("Git path is not valid UTF-8"))?;
        Ok(PathBuf::from(text))
    }
}

fn invalid(message: &str) -> Error { Error::new(ErrorKind::InvalidOutput, message) }

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ListedTree {
    pub path: PathBuf,
    pub bare: bool,
    pub prunable: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct DiscoveredTree {
    pub path: PathBuf,
    pub error: Option<Error>,
}

#[derive(Clone, Debug)]
pub(crate) struct DiscoveredRepo {
    pub path: PathBuf,
    pub trees: Vec<DiscoveredTree>,
}

pub(crate) fn worktrees(bytes: &[u8]) -> Result<Vec<ListedTree>, Error> {
    if !bytes.is_empty() && !bytes.ends_with(b"\0\0") { return Err(invalid("Incomplete worktree list record")); }
    let mut trees = Vec::new();
    let mut current: Option<ListedTree> = None;
    for field in bytes.split(|byte| *byte == 0) {
        if field.is_empty() {
            if let Some(tree) = current.take() { trees.push(tree); }
        } else if let Some(name) = field.strip_prefix(b"worktree ") {
            if current.is_some() { return Err(invalid("Worktree records lack a separator")); }
            current = Some(ListedTree { path: path(name)?, bare: false, prunable: None });
        } else {
            let tree = current.as_mut().ok_or_else(|| invalid("Worktree field before path"))?;
            if field == b"bare" { tree.bare = true; }
            if field == b"prunable" || field.starts_with(b"prunable ") {
                tree.prunable = Some(String::from_utf8_lossy(field).into_owned());
            }
        }
    }
    Ok(trees)
}

pub(crate) fn fallback_path(common: &Path) -> PathBuf {
    common.parent().filter(|_| common.file_name().is_some_and(|name| name == ".git"))
        .unwrap_or(common).to_path_buf()
}

fn missing_worktree(tree: &ListedTree) -> Option<Error> {
    if tree.path.is_dir() && tree.prunable.is_none() { return None; }
    Some(Error::new(ErrorKind::Io, format!("Missing or inaccessible worktree {}{}",
        tree.path.display(), tree.prunable.as_ref().map(|reason| format!(": {reason}")).unwrap_or_default())))
}

pub(crate) fn discover(common: &Path, cancel: &Cancel) -> Result<DiscoveredRepo, Error> {
    let mut command = args(&["--git-dir"]);
    command.push(common.as_os_str().to_owned());
    command.extend(args(&["worktree", "list", "--porcelain", "-z"]));
    let listed = worktrees(&process::checked(None, &command, cancel)?)?;
    let path = listed.first().map(|tree| tree.path.clone()).unwrap_or_else(|| fallback_path(common));
    let mut trees: Vec<_> = listed.into_iter().filter(|tree| !tree.bare).map(|tree| DiscoveredTree {
        path: tree.path.clone(),
        error: missing_worktree(&tree),
    }).collect();
    trees.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(DiscoveredRepo { path, trees })
}

fn relative_path(bytes: &[u8]) -> Result<PathBuf, Error> {
    let path = path(bytes)?;
    if path.is_absolute() || path.components().any(|part| matches!(part, std::path::Component::ParentDir)) {
        return Err(invalid("Git returned a non-relative path"));
    }
    Ok(path)
}

#[path = "../../change-counting/worktree-counts-include-branch-changes/git.rs"]
mod counting;
#[path = "../../live-monitoring/ignored-directories-do-not-cause-background-scans/git.rs"]
mod monitoring;

#[cfg(test)]
pub(crate) use counting::{category, numstat, Category};
pub(crate) use counting::scan_tree;
pub(crate) use monitoring::{global_source_candidates, global_sources, relevant_watch_paths, watch_paths, WatchPaths};

#[path = "../../snapshot-reporting/snapshots-show-counts-and-errors/scan.rs"]
mod snapshot;
pub(crate) use snapshot::scan;
