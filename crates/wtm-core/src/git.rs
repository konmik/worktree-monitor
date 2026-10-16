use crate::process::{self, args};
use crate::{Cancel, Counts, Error, ErrorKind, Head, LineCounts, RepoEntry, Snapshot, WorktreeEntry, WorktreeStats};
use std::collections::HashSet;
use std::ffi::OsString;
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

#[derive(Clone, Debug)]
pub(crate) struct WatchPaths {
    pub directories: Vec<PathBuf>,
    pub ignored_directories: HashSet<PathBuf>,
    pub tracked: Vec<PathBuf>,
}

pub(crate) fn watch_paths(tree: &Path, cancel: &Cancel) -> Result<WatchPaths, Error> {
    let tracked_output = process::checked(Some(tree), &args(&[
        "ls-files", "--full-name", "-z", "--cached",
    ]), cancel)?;
    if !tracked_output.is_empty() && !tracked_output.ends_with(b"\0") {
        return Err(invalid("Git tracked list is not NUL terminated"));
    }
    let mut tracked = Vec::new();
    for bytes in tracked_output.split(|byte| *byte == 0).filter(|bytes| !bytes.is_empty()) {
        tracked.push(relative_path(bytes)?);
    }
    tracked.sort();
    tracked.dedup();
    let (directories, ignored_directories) = watch_directories(tree, &tracked, cancel)?;
    Ok(WatchPaths { directories, ignored_directories, tracked })
}

fn watch_directories(tree: &Path, tracked: &[PathBuf], cancel: &Cancel)
    -> Result<(Vec<PathBuf>, HashSet<PathBuf>), Error> {
    let mut directories = vec![PathBuf::new()];
    let mut ignored_directories = HashSet::new();
    let mut frontier = vec![PathBuf::new()];
    while !frontier.is_empty() {
        let mut children = Vec::new();
        for relative in frontier.drain(..) {
            if cancel.is_cancelled() { return Err(Error::new(ErrorKind::Cancelled, "Git scan cancelled")); }
            let directory = tree.join(&relative);
            let entries = std::fs::read_dir(&directory)
                .map_err(|error| Error::io(format!("Cannot read {}", directory.display()), error))?;
            for entry in entries {
                if cancel.is_cancelled() { return Err(Error::new(ErrorKind::Cancelled, "Git scan cancelled")); }
                let entry = entry.map_err(|error| Error::io(format!("Cannot read {}", directory.display()), error))?;
                if !entry.file_type().map_err(|error| Error::io(format!("Cannot read {}", entry.path().display()), error))?.is_dir() {
                    continue;
                }
                let name = entry.file_name();
                if relative.as_os_str().is_empty() && name == ".git" { continue; }
                children.push(relative.join(name));
            }
        }
        children.sort();
        let ignored = check_ignored_directories(tree, &children, cancel)?;
        for child in children {
            directories.push(child.clone());
            if ignored.contains(&child) {
                ignored_directories.insert(child.clone());
            }
            if !ignored.contains(&child) || tracked.iter().any(|file| file.starts_with(&child)) {
                frontier.push(child);
            }
        }
    }
    directories.sort();
    Ok((directories, ignored_directories))
}

fn check_ignored_directories(tree: &Path, directories: &[PathBuf], cancel: &Cancel)
    -> Result<HashSet<PathBuf>, Error> {
    if directories.is_empty() {
        return Ok(HashSet::new());
    }
    let mut input = Vec::new();
    for directory in directories {
        input.extend(git_input_path(directory)?);
        input.push(0);
    }
    let output = process::run_with_input(Some(tree), &args(&[
        "check-ignore", "--stdin", "-z", "--no-index",
    ]), &input, cancel)?;
    let output = process::success(output, &[0, 1])?;
    output.split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty()).map(relative_path).collect()
}

fn git_input_path(path: &Path) -> Result<Vec<u8>, Error> {
    #[cfg(windows)]
    { path.to_str().map(|path| path.replace('\\', "/").into_bytes())
        .ok_or_else(|| invalid("Git path is not valid UTF-8")) }
    #[cfg(not(windows))]
    { Ok(std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str()).to_vec()) }
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

#[derive(PartialEq, Eq)]
struct ScanIdentity {
    head: Head,
    commit: String,
    base_ref: Option<(&'static str, String)>,
}

fn scan_identity(tree: &Path, cancel: &Cancel) -> Result<ScanIdentity, Error> {
    let (head, commit) = head(tree, cancel)?;
    let mut base_ref = None;
    for reference in ["refs/heads/main", "refs/heads/master"] {
        let output = process::run(Some(tree), &args(&["show-ref", "--verify", "--quiet", reference]), cancel)?;
        let code = output.code;
        process::success(output, &[0, 1])?;
        if code == 0 {
            let oid = process::checked(Some(tree), &args(&["rev-parse", "--verify", &format!("{reference}^{{commit}}")]), cancel)?;
            base_ref = Some((reference, text(&oid)?));
            break;
        }
    }
    Ok(ScanIdentity { head, commit, base_ref })
}

fn comparison_base(tree: &Path, identity: &ScanIdentity, cancel: &Cancel) -> Result<String, Error> {
    if !matches!(identity.head, Head::Unborn(_)) {
        if let Some((_, base)) = &identity.base_ref {
            let output = process::run(Some(tree), &args(&["merge-base", &identity.commit, base]), cancel)?;
            let code = output.code;
            let stdout = process::success(output, &[0, 1])?;
            if code == 0 { return text(&stdout); }
        }
    }
    Ok(identity.commit.clone())
}

pub(crate) fn scan_tree(tree: &Path, cancel: &Cancel) -> Result<WorktreeStats, Error> {
    for _ in 0..2 {
        let identity = scan_identity(tree, cancel)?;
        let base = comparison_base(tree, &identity, cancel)?;
        let stats = count_tree(tree, identity.head.clone(), &base, cancel)?;
        if scan_identity(tree, cancel)? == identity {
            return Ok(stats);
        }
    }
    Err(Error::new(ErrorKind::ChangedDuringScan, "HEAD or comparison branch changed during the scan; retry on the next scan"))
}

pub(crate) fn global_sources(tree: &Path, cancel: &Cancel) -> Result<Vec<PathBuf>, Error> {
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
            } else if key == b"core.attributesfile" {
                if let Some(value) = setting.next().filter(|value| !value.is_empty()) {
                    sources.push(source_path(tree, path(value)?));
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
        sources.push(xdg_config.join("git/attributes"));
        if xdg_config != home_config { sources.push(home_config.join("git/ignore")); }
        if xdg_config != home_config { sources.push(home_config.join("git/attributes")); }
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

const UNTRACKED_BATCH_LIMIT: usize = 16 * 1024;
const LITERAL_PATHSPEC_PREFIX: &str = ":(literal)";
const PATHSPEC_QUOTING_MARGIN: usize = 4;

fn count_untracked(tree: &Path, relative: &Path, cancel: &Cancel) -> Result<Counts, Error> {
    let mut command = args(&["diff", "--no-index", "--numstat", "-z", "--no-ext-diff", "--no-textconv", "--", "/dev/null"]);
    command.push(relative.as_os_str().to_owned());
    let output = process::success(process::run(Some(tree), &command, cancel)?, &[0, 1])?;
    numstat(&output)
}

fn count_untracked_batch(tree: &Path, empty: &Path, relatives: &[PathBuf], cancel: &Cancel) -> Result<Counts, Error> {
    let mut command = args(&["diff", "--no-index", "--numstat", "-z", "--no-ext-diff", "--no-textconv"]);
    command.push(empty.as_os_str().to_owned());
    command.push(".".into());
    command.push("--".into());
    for relative in relatives {
        let mut pathspec = OsString::from(LITERAL_PATHSPEC_PREFIX);
        pathspec.push(relative.as_os_str());
        command.push(pathspec);
    }
    let output = process::run(Some(tree), &command, cancel)?;
    if output.code == 129 {
        let mut counts = Counts::default();
        for relative in relatives {
            counts.add(count_untracked(tree, relative, cancel)?)?;
        }
        return Ok(counts);
    }
    let output = process::success(output, &[0, 1])?;
    numstat(&output)
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
    let mut relatives = Vec::new();
    for name in untracked.split(|byte| *byte == 0).filter(|name| !name.is_empty()) {
        let relative = relative_path(name)?;
        let absolute = tree.join(&relative);
        let metadata = std::fs::symlink_metadata(&absolute)
            .map_err(|error| Error::io(format!("Cannot read {}", absolute.display()), error))?;
        if !metadata.is_file() && !metadata.file_type().is_symlink() {
            return Err(Error::new(ErrorKind::Io, format!("Unsupported untracked entry {}", absolute.display())));
        }
        relatives.push(relative);
    }
    match relatives.as_slice() {
        [] => {}
        [relative] => counts.add(count_untracked(tree, relative, cancel)?)?,
        _ => {
            let empty = tempfile::tempdir().map_err(|error| Error::io("Cannot create Git comparison directory", error))?;
            let mut start = 0;
            while start < relatives.len() {
                let mut end = start;
                let mut size = 0;
                while end < relatives.len() {
                    let pathspec_size = LITERAL_PATHSPEC_PREFIX.len()
                        + relatives[end].as_os_str().as_encoded_bytes().len()
                        + PATHSPEC_QUOTING_MARGIN;
                    if end > start && size + pathspec_size > UNTRACKED_BATCH_LIMIT { break; }
                    size += pathspec_size;
                    end += 1;
                }
                counts.add(count_untracked_batch(tree, empty.path(), &relatives[start..end], cancel)?)?;
                start = end;
            }
        }
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
        let directory = tempfile::Builder::new().prefix("wtm-ignore-config-").tempdir_in(parent).unwrap();
        let tree = directory.path();
        let cancel = Cancel::new();
        process::checked(Some(tree), &args(&["init", "--quiet"]), &cancel).unwrap();
        let included = tree.join(".git/included config");
        std::fs::write(&included, b"").unwrap();
        let mut command = args(&["config", "include.path"]);
        command.push("included config".into());
        process::checked(Some(tree), &command, &cancel).unwrap();
        let sources = global_sources(tree, &cancel).unwrap();
        assert!(sources.iter().any(|source| crate::poll::normalize(source) == crate::poll::normalize(&included)));
    }

    #[test]
    fn watch_paths_checks_ignores_before_descending_and_lists_cached_paths_only() {
        let directory = tempfile::tempdir().unwrap();
        let tree = directory.path().join("repo");
        std::fs::create_dir(&tree).unwrap();
        let cancel = Cancel::new();
        process::checked(Some(&tree), &args(&["init", "--quiet"]), &cancel).unwrap();
        std::fs::write(tree.join(".gitignore"), b"ignored/\n").unwrap();
        std::fs::create_dir_all(tree.join("ignored/noise/deep")).unwrap();
        std::fs::create_dir_all(tree.join("ignored/disposable/hidden/deep")).unwrap();
        std::fs::create_dir(tree.join("empty")).unwrap();
        std::fs::write(tree.join("ignored/noise/deep/tracked.txt"), b"tracked\n").unwrap();
        process::checked(Some(&tree), &args(&["add", ".gitignore"]), &cancel).unwrap();
        process::checked(Some(&tree), &args(&["add", "-f", "ignored/noise/deep/tracked.txt"]), &cancel).unwrap();
        std::fs::write(tree.join("untracked.txt"), b"untracked\n").unwrap();
        let paths = watch_paths(&tree, &cancel).unwrap();
        assert!(paths.directories.contains(&PathBuf::from("empty")));
        assert!(paths.directories.contains(&PathBuf::from("ignored")));
        assert!(paths.directories.contains(&PathBuf::from("ignored/noise")));
        assert!(paths.directories.contains(&PathBuf::from("ignored/noise/deep")));
        assert!(paths.ignored_directories.contains(&PathBuf::from("ignored")));
        assert!(paths.ignored_directories.contains(&PathBuf::from("ignored/noise")));
        assert!(paths.tracked.contains(&PathBuf::from(".gitignore")));
        assert!(paths.tracked.contains(&PathBuf::from("ignored/noise/deep/tracked.txt")));
        assert!(!paths.tracked.contains(&PathBuf::from("untracked.txt")));
        assert!(!paths.directories.contains(&PathBuf::from("ignored/disposable/hidden")));
    }

    #[test]
    fn watch_paths_preserves_broad_directory_coverage_and_prunes_ignored_children() {
        let directory = tempfile::tempdir().unwrap();
        let tree = directory.path();
        let cancel = Cancel::new();
        process::checked(Some(tree), &args(&["init", "--quiet"]), &cancel).unwrap();
        std::fs::write(tree.join(".gitignore"), b"ignored/\n").unwrap();
        for index in 0..48 {
            std::fs::create_dir_all(tree.join(format!("branch-{index}/visible/deep"))).unwrap();
            std::fs::create_dir_all(tree.join(format!("branch-{index}/ignored/hidden"))).unwrap();
        }
        let paths = watch_paths(tree, &cancel).unwrap();
        assert_eq!(paths.directories.len(), 1 + 48 * 4);
        assert_eq!(paths.ignored_directories.len(), 48);
        for index in 0..48 {
            assert!(paths.directories.contains(&PathBuf::from(format!("branch-{index}/visible/deep"))));
            assert!(paths.ignored_directories.contains(&PathBuf::from(format!("branch-{index}/ignored"))));
            assert!(!paths.directories.contains(&PathBuf::from(format!("branch-{index}/ignored/hidden"))));
        }
    }

    #[test]
    fn cancelled_directory_walk_stops_before_reading_the_next_directory() {
        let directory = tempfile::tempdir().unwrap();
        let cancel = Cancel::new();
        cancel.cancel();
        let error = watch_directories(&directory.path().join("missing"), &[], &cancel).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Cancelled);
    }

    #[test]
    fn watch_paths_preserves_negated_directory_listing() {
        let directory = tempfile::tempdir().unwrap();
        let tree = directory.path().join("repo");
        std::fs::create_dir(&tree).unwrap();
        let cancel = Cancel::new();
        process::checked(Some(&tree), &args(&["init", "--quiet"]), &cancel).unwrap();
        std::fs::write(tree.join(".gitignore"), b"listed/*\n!listed/visible.txt\n").unwrap();
        std::fs::create_dir_all(tree.join("listed/nested")).unwrap();
        let paths = watch_paths(&tree, &cancel).unwrap();
        assert!(paths.directories.contains(&PathBuf::from("listed")));
        assert!(paths.directories.contains(&PathBuf::from("listed/nested")));
        assert!(paths.ignored_directories.contains(&PathBuf::from("listed/nested")));
    }

    #[test]
    fn configured_attributes_are_retained_as_monitoring_sources() {
        let directory = tempfile::tempdir().unwrap();
        let tree = directory.path();
        let cancel = Cancel::new();
        process::checked(Some(tree), &args(&["init", "--quiet"]), &cancel).unwrap();
        process::checked(Some(tree), &args(&["config", "core.attributesFile", "ignored/rules"]), &cancel).unwrap();
        let sources = global_sources(tree, &cancel).unwrap();
        assert!(sources.iter().any(|source| crate::poll::normalize(source)
            == crate::poll::normalize(&tree.join("ignored/rules"))));
    }
}
