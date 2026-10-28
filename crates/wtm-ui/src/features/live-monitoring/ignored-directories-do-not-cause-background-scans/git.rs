use crate::process::{self, args};
use crate::{Cancel, Error, ErrorKind};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use super::{invalid, path, relative_path};

#[derive(Clone, Debug)]
pub(crate) struct WatchPaths {
    pub directories: Vec<PathBuf>,
    pub ignored_directories: HashSet<PathBuf>,
    pub tracked: Vec<PathBuf>,
}

pub(crate) fn relevant_watch_paths(tree: &Path, paths: &HashSet<PathBuf>, cancel: &Cancel) -> Result<bool, Error> {
    let mut input = Vec::new();
    let mut queries = Vec::new();
    for path in paths {
        let query = git_input_path(path)?;
        queries.push(query.clone());
        if std::fs::symlink_metadata(tree.join(path)).is_err() && !query.ends_with(b"/") {
            let mut directory = query;
            directory.push(b'/');
            queries.push(directory);
        }
    }
    for query in &queries {
        input.extend(query);
        input.push(0);
    }
    let output = process::run_with_input(Some(tree), &args(&[
        "check-ignore", "--stdin", "-z",
    ]), &input, cancel)?;
    let output = process::success(output, &[0, 1])?;
    if !output.is_empty() && !output.ends_with(b"\0") {
        return Err(invalid("Git ignored path list is not NUL terminated"));
    }
    let ignored: HashSet<_> = output.split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty()).map(|bytes| relative_path(bytes).map(|_| bytes.to_vec()))
        .collect::<Result<_, _>>()?;
    Ok(queries.iter().any(|query| !ignored.contains(query)))
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

#[derive(Clone, Debug, Default)]
pub(crate) struct MonitoringSources {
    pub(crate) global: Vec<PathBuf>,
    pub(crate) repository: Vec<PathBuf>,
}

pub(crate) fn monitoring_sources(tree: &Path, cancel: &Cancel) -> Result<MonitoringSources, Error> {
    let configuration = process::checked(Some(tree),
        &args(&["config", "--null", "--show-scope", "--show-origin", "--path", "--list"]), cancel)?;
    let mut sources = configuration_sources(tree, &configuration)?;
    let output = process::run(Some(tree),
        &args(&["config", "--null", "--show-scope", "--show-origin", "--path", "--get", "core.excludesFile"]), cancel)?;
    let code = output.code;
    let stdout = process::success(output, &[0, 1])?;
    if code == 1 && sources.global.is_empty() && sources.repository.is_empty() {
        return Err(invalid("Could not locate Git monitoring sources"));
    }
    if code == 0 {
        let fields: Vec<_> = stdout.split(|byte| *byte == 0).collect();
        if fields.len() != 4 || !fields[3].is_empty() {
            return Err(invalid("Git excludes configuration is malformed"));
        }
        let scoped = scoped_sources(&mut sources, fields[0])?;
        if !fields[2].is_empty() { scoped.push(source_path(tree, path(fields[2])?)); }
        if let Some(origin) = fields[1].strip_prefix(b"file:") {
            scoped.push(source_path(tree, path(origin)?));
        }
    }
    for scoped in [&mut sources.global, &mut sources.repository] {
        scoped.sort();
        scoped.dedup();
    }
    Ok(sources)
}

fn configuration_sources(tree: &Path, configuration: &[u8]) -> Result<MonitoringSources, Error> {
    let mut sources = MonitoringSources { global: global_source_candidates(), repository: Vec::new() };
    if !configuration.is_empty() && !configuration.ends_with(b"\0") {
        return Err(invalid("Git configuration origins are not NUL terminated"));
    }
    let records: Vec<_> = configuration.split(|byte| *byte == 0).collect();
    let records = &records[..records.len() - 1];
    if records.len() % 3 != 0 { return Err(invalid("Git configuration origins are incomplete")); }
    for record in records.chunks_exact(3) {
        if let Some(origin) = record[1].strip_prefix(b"file:") {
            let origin = source_path(tree, path(origin)?);
            let scoped = scoped_sources(&mut sources, record[0])?;
            let mut setting = record[2].splitn(2, |byte| *byte == b'\n');
            let key = setting.next().unwrap();
            if key == b"include.path" || key.starts_with(b"includeif.") && key.ends_with(b".path") {
                if let Some(value) = setting.next().filter(|value| !value.is_empty()) {
                    scoped.push(source_path(origin.parent().unwrap_or(tree), path(value)?));
                }
            } else if key == b"core.attributesfile" {
                if let Some(value) = setting.next().filter(|value| !value.is_empty()) {
                    scoped.push(source_path(tree, path(value)?));
                }
            }
            scoped.push(origin);
        }
    }
    Ok(sources)
}

fn scoped_sources<'a>(sources: &'a mut MonitoringSources, scope: &[u8]) -> Result<&'a mut Vec<PathBuf>, Error> {
    match scope {
        b"system" | b"global" => Ok(&mut sources.global),
        b"local" | b"worktree" | b"command" => Ok(&mut sources.repository),
        _ => Err(invalid("Git configuration scope is invalid")),
    }
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

#[cfg(test)]
#[path = "git.test.rs"]
mod tests;
