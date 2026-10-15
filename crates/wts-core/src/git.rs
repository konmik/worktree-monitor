use crate::process::{self, args};
use crate::{Cancel, Counts, Error, ErrorKind, Head, LineCounts, RepoEntry, Snapshot, WorktreeEntry, WorktreeStats};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Category { Source, Test, Extra }

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum IgnoredPath {
    File(PathBuf),
    Directory(PathBuf),
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct IgnoredPaths {
    entries: Vec<IgnoredPath>,
    checked_directories: Vec<PathBuf>,
    sources: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IgnoredPathKind { File, Directory }

impl IgnoredPaths {
    pub(crate) fn sources(&self) -> &[PathBuf] { &self.sources }

    pub(crate) fn matching(&self, path: &Path) -> Option<(IgnoredPathKind, bool)> {
        let path = ignored_path_key(path);
        self.entries.iter().find_map(|entry| match entry {
            IgnoredPath::File(candidate) if path == ignored_path_key(candidate) => {
                Some((IgnoredPathKind::File, true))
            }
            IgnoredPath::Directory(candidate) => {
                let candidate = ignored_path_key(candidate);
                if path == candidate {
                    Some((IgnoredPathKind::Directory, true))
                } else if path.starts_with(&candidate) {
                    Some((IgnoredPathKind::Directory, false))
                } else {
                    None
                }
            }
            _ => None,
        })
    }
}

#[derive(Debug)]
pub(crate) struct ScanResult {
    pub(crate) stats: WorktreeStats,
    pub(crate) ignored: Option<IgnoredPaths>,
}

fn ignored_path_key(path: &Path) -> PathBuf {
    #[cfg(windows)]
    { PathBuf::from(path.to_string_lossy().replace('\\', "/").to_lowercase()) }
    #[cfg(not(windows))]
    { path.to_path_buf() }
}

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
    scan_tree_inner(tree, cancel, false, None).map(|result| result.stats)
}

pub(crate) fn scan_tree_with_ignored(tree: &Path, cancel: &Cancel,
    previous: Option<&IgnoredPaths>) -> Result<ScanResult, Error> {
    scan_tree_inner(tree, cancel, true, previous)
}

fn scan_tree_inner(tree: &Path, cancel: &Cancel, collect_ignored: bool,
    previous: Option<&IgnoredPaths>) -> Result<ScanResult, Error> {
    for _ in 0..2 {
        let (head, base) = head(tree, cancel)?;
        let stats = count_tree(tree, head.clone(), &base, cancel)?;
        let ignored = if collect_ignored {
            match ignored_paths(tree, cancel, previous) {
                Ok(paths) => Some(paths),
                Err(error) if error.kind == ErrorKind::Cancelled => return Err(error),
                Err(_) => None,
            }
        } else { None };
        if self::head(tree, cancel)? == (head, base) {
            return Ok(ScanResult { stats, ignored });
        }
    }
    Err(Error::new(ErrorKind::ChangedDuringScan, "HEAD changed during the scan; retry on the next scan"))
}

fn ignored_paths(tree: &Path, cancel: &Cancel, previous: Option<&IgnoredPaths>) -> Result<IgnoredPaths, Error> {
    let sources = match previous {
        Some(previous) => previous.sources.clone(),
        None => global_sources(tree, cancel)?,
    };
    let ignored = process::checked(Some(tree),
        &args(&["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "-z"]), cancel)?;
    if !ignored.is_empty() && !ignored.ends_with(b"\0") {
        return Err(invalid("Git ignored list is not NUL terminated"));
    }
    let mut entries = ignored.split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| {
            let directory = name.ends_with(b"/");
            let name = if directory { &name[..name.len() - 1] } else { name };
            let path = relative_path(name)?;
            Ok(if directory { IgnoredPath::Directory(path) } else { IgnoredPath::File(path) })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    if entries.iter().any(|entry| matches!(entry, IgnoredPath::Directory(_))) {
        let tracked = process::checked(Some(tree), &args(&["ls-files", "--cached", "-z"]), cancel)?;
        if !tracked.is_empty() && !tracked.ends_with(b"\0") {
            return Err(invalid("Git tracked list is not NUL terminated"));
        }
        let tracked = tracked.split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .map(relative_path)
            .map(|path| path.map(|path| ignored_path_key(&path)))
            .collect::<Result<Vec<_>, Error>>()?;
        entries.retain(|entry| match entry {
            IgnoredPath::Directory(directory) => {
                let directory = ignored_path_key(directory);
                !tracked.iter().any(|path| path.starts_with(&directory))
            }
            IgnoredPath::File(_) => true,
        });
    }
    let mut checked_directories = Vec::new();
    let mut ignored_directories = HashSet::new();
    let mut input = Vec::new();
    for entry in &entries {
        if let IgnoredPath::Directory(directory) = entry {
            let directory_key = ignored_path_key(directory);
            checked_directories.push(directory.clone());
            if previous.is_some_and(|previous| previous.checked_directories.iter()
                .any(|path| ignored_path_key(path) == directory_key)) {
                if previous.is_some_and(|previous| previous.entries.iter().any(|entry|
                    matches!(entry, IgnoredPath::Directory(path)
                        if ignored_path_key(path) == directory_key))) {
                    ignored_directories.insert(directory_key);
                }
            } else {
                input.extend_from_slice(directory.as_os_str().as_encoded_bytes());
                input.push(0);
            }
        }
    }
    if !input.is_empty() {
        let output = process::run_with_input(Some(tree),
            &args(&["check-ignore", "--stdin", "-z", "--no-index"]), cancel, &input)?;
        let ignored = process::success(output, &[0, 1])?;
        if !ignored.is_empty() && !ignored.ends_with(b"\0") {
            return Err(invalid("Git checked ignored paths are not NUL terminated"));
        }
        for name in ignored.split(|byte| *byte == 0).filter(|name| !name.is_empty()) {
            ignored_directories.insert(ignored_path_key(&relative_path(name)?));
        }
    }
    entries.retain(|entry| match entry {
        IgnoredPath::Directory(directory) => ignored_directories.contains(&ignored_path_key(directory)),
        IgnoredPath::File(_) => true,
    });
    entries.sort();
    entries.dedup();
    checked_directories.sort();
    checked_directories.dedup();
    Ok(IgnoredPaths { entries, checked_directories, sources })
}

fn global_sources(tree: &Path, cancel: &Cancel) -> Result<Vec<PathBuf>, Error> {
    let mut sources = global_source_candidates();
    let configuration = process::checked(Some(tree),
        &args(&["config", "--null", "--show-origin", "--path", "--list"]), cancel)?;
    if !configuration.is_empty() && !configuration.ends_with(b"\0") {
        return Err(invalid("Git configuration origins are not NUL terminated"));
    }
    let records: Vec<_> = configuration.split(|byte| *byte == 0).collect();
    let records = &records[..records.len() - 1];
    if records.len() % 2 != 0 { return Err(invalid("Git configuration origins are incomplete")); }
    for record in records.chunks_exact(2) {
        if let Some(origin) = record[0].strip_prefix(b"file:") {
            let origin = source_path(tree, path(origin)?);
            let mut setting = record[1].splitn(2, |byte| *byte == b'\n');
            let key = setting.next().unwrap();
            if key == b"include.path" || key.starts_with(b"includeif.") && key.ends_with(b".path") {
                if let Some(value) = setting.next().filter(|value| !value.is_empty()) {
                    sources.push(source_path(origin.parent().unwrap_or(tree), path(value)?));
                }
            }
            sources.push(origin);
        }
    }
    let output = process::run(Some(tree),
        &args(&["config", "--null", "--show-origin", "--path", "--get", "core.excludesFile"]), cancel)?;
    let code = output.code;
    let stdout = process::success(output, &[0, 1])?;
    if code == 1 && sources.is_empty() {
        return Err(invalid("Could not locate the global Git excludes file"));
    }
    if code == 0 {
        let fields: Vec<_> = stdout.split(|byte| *byte == 0).collect();
        if fields.len() != 3 || !fields[2].is_empty() {
            return Err(invalid("Git global excludes configuration is malformed"));
        }
        if !fields[1].is_empty() { sources.push(source_path(tree, path(fields[1])?)); }
        if let Some(origin) = fields[0].strip_prefix(b"file:") {
            sources.push(source_path(tree, path(origin)?));
        }
    }
    sources.sort();
    sources.dedup();
    Ok(sources)
}

pub(crate) fn global_source_candidates() -> Vec<PathBuf> {
    let mut sources = Vec::new();
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|home| !home.is_empty()).map(PathBuf::from) {
        let home_config = home.join(".config");
        let xdg_config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|path| !path.is_empty()).map(PathBuf::from).unwrap_or_else(|| home_config.clone());
        sources.push(xdg_config.join("git/ignore"));
        if xdg_config != home_config { sources.push(home_config.join("git/ignore")); }
        sources.push(home.join(".gitconfig"));
        sources.push(xdg_config.join("git/config"));
        if xdg_config != home_config { sources.push(home_config.join("git/config")); }
    }
    for variable in ["GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM"] {
        if let Some(path) = std::env::var_os(variable).filter(|path| !path.is_empty()) {
            sources.push(PathBuf::from(path));
        }
    }
    sources.sort();
    sources.dedup();
    sources
}

fn source_path(tree: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() { path } else { tree.join(path) }
}

fn relative_path(bytes: &[u8]) -> Result<PathBuf, Error> {
    let path = path(bytes)?;
    if path.is_absolute() || path.components().any(|part| matches!(part, std::path::Component::ParentDir)) {
        return Err(invalid("Git returned a non-relative path"));
    }
    Ok(path)
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
        let relative = relative_path(name)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn included_configuration_is_watched_before_it_sets_excludes() {
        let parent = std::env::temp_dir().join("opencode");
        std::fs::create_dir_all(&parent).unwrap();
        let directory = tempfile::Builder::new().prefix("wts-ignore-config-").tempdir_in(parent).unwrap();
        let tree = directory.path();
        let cancel = Cancel::new();
        process::checked(Some(tree), &args(&["init", "--quiet"]), &cancel).unwrap();
        let included = tree.join(".git/included config");
        std::fs::write(&included, b"").unwrap();
        let mut command = args(&["config", "include.path"]);
        command.push("included config".into());
        process::checked(Some(tree), &command, &cancel).unwrap();
        let sources = global_sources(tree, &cancel).unwrap();
        assert!(sources.iter().any(|source| ignored_path_key(source) == ignored_path_key(&included)));
    }
}
