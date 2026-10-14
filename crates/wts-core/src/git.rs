use crate::process::{self, args};
use crate::{Cancel, Counts, Error, ErrorKind, Head, LineCounts, RepoEntry, Snapshot, WorktreeEntry, WorktreeStats};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Category { Source, Test, Extra }

const TEST_DIRECTORIES: &[&str] = &[
    "test", "tests", "__tests__", "e2e", "spec", "testdata", "__mocks__", "__fixtures__", "fixtures", "__snapshots__",
];

const EXTRA_DIRECTORIES: &[&str] = &["tools", "scripts", "development_tools", "development-tools", "tooling"];

pub(crate) fn category(path: &Path) -> Category {
    let normalized = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let parts: Vec<_> = normalized.split('/').collect();
    if parts.iter().any(|part| TEST_DIRECTORIES.contains(part)) {
        return Category::Test;
    }
    let name = parts.last().copied().unwrap_or_default();
    if name.starts_with("test-") || name.starts_with("test_") || name.contains(".test.")
        || name.contains(".spec.") || name.contains("_test.")
        || ["ava", "cypress", "jest", "karma", "playwright", "vitest"].iter()
            .any(|prefix| name.starts_with(prefix) && name[prefix.len()..].starts_with(".config."))
        || ["test", "tests"].iter().any(|prefix| name.starts_with(prefix)
            && (name[prefix.len()..].starts_with(".config.") || name[prefix.len()..].starts_with(".setup."))) {
        return Category::Test;
    }
    if parts.iter().any(|part| EXTRA_DIRECTORIES.contains(part))
        || [".ps1", ".sh", ".bash", ".bat", ".cmd"].iter().any(|suffix| name.ends_with(suffix)) {
        return Category::Extra;
    }
    Category::Source
}

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

pub(crate) fn numstat(bytes: &[u8]) -> Result<Counts, Error> {
    if !bytes.is_empty() && !bytes.ends_with(b"\0") { return Err(invalid("Git numstat is not NUL terminated")); }
    let mut fields = bytes.split(|byte| *byte == 0).peekable();
    let mut counts = Counts::default();
    while let Some(record) = fields.next() {
        if record.is_empty() && fields.peek().is_none() { break; }
        let mut columns = record.splitn(3, |byte| *byte == b'\t');
        let additions = columns.next().ok_or_else(|| invalid("Missing numstat additions"))?;
        let deletions = columns.next().ok_or_else(|| invalid("Missing numstat deletions"))?;
        let name = columns.next().ok_or_else(|| invalid("Missing numstat path"))?;
        let destination = if name.is_empty() {
            let old = fields.next().ok_or_else(|| invalid("Incomplete numstat rename"))?;
            let destination = fields.next().ok_or_else(|| invalid("Incomplete numstat rename"))?;
            if old.is_empty() || destination.is_empty() { return Err(invalid("Incomplete numstat rename")); }
            destination
        } else { name };
        let category = category(&path(destination)?);
        let entry = if additions == b"-" && deletions == b"-" {
            Counts { binaries: 1, ..Counts::default() }
        } else {
            let number = |bytes: &[u8]| -> Result<u64, Error> {
                if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) { return Err(invalid("Invalid numstat count")); }
                std::str::from_utf8(bytes).unwrap().parse().map_err(|_| invalid("Numstat count overflow"))
            };
            let lines = LineCounts { additions: number(additions)?, deletions: number(deletions)? };
            match category {
                Category::Source => Counts { source: lines, ..Counts::default() },
                Category::Test => Counts { test: lines, ..Counts::default() },
                Category::Extra => Counts { extra: lines, ..Counts::default() },
            }
        };
        counts.add(entry)?;
    }
    Ok(counts)
}

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

fn fallback_path(common: &Path) -> PathBuf {
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

fn text(bytes: &[u8]) -> Result<String, Error> {
    std::str::from_utf8(bytes).map(|text| text.trim_end_matches(['\r', '\n']).to_owned())
        .map_err(|_| invalid("Git identity is not valid UTF-8"))
}

fn head(tree: &Path, cancel: &Cancel) -> Result<(Head, String), Error> {
    let symbolic = process::run(Some(tree), &args(&["symbolic-ref", "--quiet", "HEAD"]), cancel)?;
    let branch = match symbolic.code {
        0 => Some(text(&symbolic.stdout)?.strip_prefix("refs/heads/")
            .ok_or_else(|| invalid("HEAD does not name a local branch"))?.to_owned()),
        1 => None,
        _ => { process::success(symbolic, &[0, 1])?; unreachable!() },
    };
    let commit = process::run(Some(tree), &args(&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]), cancel)?;
    if commit.code == 0 {
        let oid = text(&commit.stdout)?;
        return Ok((branch.map(Head::Branch).unwrap_or_else(|| Head::Detached(oid.clone())), oid));
    }
    if let Some(branch) = branch {
        let mut command = args(&["show-ref", "--verify", "--quiet"]);
        command.push(format!("refs/heads/{branch}").into());
        let reference = process::run(Some(tree), &command, cancel)?;
        if reference.code == 1 {
            let empty = process::checked(Some(tree), &args(&["hash-object", "-t", "tree", "--stdin"]), cancel)?;
            return Ok((Head::Unborn(branch), text(&empty)?));
        }
        process::success(reference, &[0])?;
    }
    process::success(commit, &[0])?;
    Err(invalid("Cannot resolve HEAD"))
}

pub(crate) fn scan_tree(tree: &Path, cancel: &Cancel) -> Result<WorktreeStats, Error> {
    for _ in 0..2 {
        let (head, base) = head(tree, cancel)?;
        let stats = count_tree(tree, head.clone(), &base, cancel)?;
        if self::head(tree, cancel)? == (head, base) { return Ok(stats); }
    }
    Err(Error::new(ErrorKind::ChangedDuringScan, "HEAD changed during the scan; retry on the next scan"))
}

fn count_tree(tree: &Path, head: Head, base: &str, cancel: &Cancel) -> Result<WorktreeStats, Error> {
    let conflicts = process::checked(Some(tree), &args(&["ls-files", "--unmerged", "-z"]), cancel)?;
    if !conflicts.is_empty() { return Err(Error::new(ErrorKind::Conflict, "Unresolved merge conflicts")); }
    let mut command = args(&["diff", "--numstat", "-z", "--no-ext-diff", "--no-textconv", "--find-renames", "--ignore-submodules=all"]);
    command.push(base.into());
    command.push("--".into());
    let mut counts = numstat(&process::checked(Some(tree), &command, cancel)?)?;
    let untracked = process::checked(Some(tree), &args(&["ls-files", "--others", "--exclude-standard", "-z"]), cancel)?;
    if !untracked.is_empty() && !untracked.ends_with(b"\0") { return Err(invalid("Untracked list is not NUL terminated")); }
    for name in untracked.split(|byte| *byte == 0).filter(|name| !name.is_empty()) {
        let relative = path(name)?;
        if relative.is_absolute() || relative.components().any(|part| matches!(part, std::path::Component::ParentDir)) {
            return Err(invalid("Git returned a non-relative untracked path"));
        }
        let absolute = tree.join(&relative);
        let metadata = std::fs::symlink_metadata(&absolute)
            .map_err(|error| Error::io(format!("Cannot read {}", absolute.display()), error))?;
        if !metadata.is_file() && !metadata.file_type().is_symlink() {
            return Err(Error::new(ErrorKind::Io, format!("Unsupported untracked entry {}", absolute.display())));
        }
        let mut command = args(&["diff", "--no-index", "--numstat", "-z", "--no-ext-diff", "--no-textconv", "--", "/dev/null"]);
        command.push(relative.into_os_string());
        let output = process::success(process::run(Some(tree), &command, cancel)?, &[0, 1])?;
        counts.add(numstat(&output)?)?;
    }
    Ok(WorktreeStats { head, counts })
}
