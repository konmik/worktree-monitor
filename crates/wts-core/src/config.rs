use crate::process::{self, args};
use crate::{Cancel, Error, ErrorKind};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    repos: Vec<PathBuf>,
}

pub(crate) fn read(path: &Path) -> Result<Vec<u8>, Error> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(Error::io(format!("Cannot read configuration {}", path.display()), error)),
    };
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)
        .map_err(|error| Error::io("Cannot read configuration", error))?;
    if bytes.len() > 1024 * 1024 { return Err(Error::config("Configuration exceeds 1 MiB")); }
    if bytes.is_empty() { return Err(Error::config("Configuration file is empty")); }
    Ok(bytes)
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Vec<PathBuf>, Error> {
    if bytes.is_empty() { return Ok(Vec::new()); }
    let registry: Registry = serde_json::from_slice(bytes)
        .map_err(|error| Error::config(format!("Invalid configuration: {error}")))?;
    if registry.repos.iter().any(|path| !path.is_absolute()) {
        return Err(Error::config("Registered Git directories must be absolute paths"));
    }
    let mut repos = registry.repos;
    repos.sort_by_key(|path| identity(path));
    repos.dedup_by(|left, right| identity(left) == identity(right));
    Ok(repos)
}

fn identity(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) { text.to_lowercase() } else { text.into_owned() }
}

pub(crate) fn resolve(path: &Path, cancel: &Cancel) -> Result<PathBuf, Error> {
    let output = process::checked(Some(path), &args(&["rev-parse", "--path-format=absolute", "--git-common-dir"]), cancel)?;
    let output = output.strip_suffix(b"\n").unwrap_or(&output);
    let output = output.strip_suffix(b"\r").unwrap_or(output);
    let common = crate::git::path(output)?;
    dunce::canonicalize(&common).map_err(|error| Error::io(format!("Cannot resolve {}", common.display()), error))
}

fn absolute(path: &Path) -> Result<PathBuf, Error> {
    let path = if path.is_absolute() { path.to_path_buf() } else {
        std::env::current_dir().map_err(|error| Error::io("Cannot resolve current directory", error))?.join(path)
    };
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {},
            Component::ParentDir => { result.pop(); },
            other => result.push(other.as_os_str()),
        }
    }
    Ok(result)
}

pub(crate) fn update(config: &Path, path: &Path, add: bool, cancel: &Cancel) -> Result<bool, Error> {
    let config = absolute(config)?;
    let parent = config.parent().ok_or_else(|| Error::config("Configuration has no parent directory"))?;
    fs::create_dir_all(parent).map_err(|error| Error::io("Cannot create configuration directory", error))?;
    let lock = OpenOptions::new().create(true).truncate(false).read(true).write(true)
        .open(config.with_extension("lock")).map_err(|error| Error::io("Cannot open configuration lock", error))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match lock.try_lock_exclusive() {
            Ok(()) => break,
            Err(error) if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
                if cancel.is_cancelled() { return Err(Error::new(ErrorKind::Cancelled, "Configuration update cancelled")); }
                if Instant::now() >= deadline { return Err(Error::new(ErrorKind::Timeout, "Configuration lock timed out after 5 seconds")); }
                cancel.wait(Duration::from_millis(25));
            }
            Err(error) => return Err(Error::io("Cannot lock configuration", error)),
        }
    }
    let mut repos = parse(&read(&config)?)?;
    let common = match resolve(path, cancel) {
        Ok(common) => common,
        Err(error) if add => return Err(error),
        Err(_) => absolute(path)?,
    };
    let before = repos.clone();
    if add {
        if !repos.iter().any(|repo| identity(repo) == identity(&common)) { repos.push(common); }
    } else {
        let root_git = common.join(".git");
        repos.retain(|repo| identity(repo) != identity(&common) && identity(repo) != identity(&root_git));
    }
    repos.sort_by_key(|path| identity(path));
    if repos == before { return Ok(false); }
    let bytes = serde_json::to_vec_pretty(&Registry { repos })
        .map_err(|error| Error::config(format!("Cannot encode configuration: {error}")))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| Error::io("Cannot create temporary configuration", error))?;
    temporary.write_all(&bytes).map_err(|error| Error::io("Cannot write configuration", error))?;
    temporary.flush().map_err(|error| Error::io("Cannot flush configuration", error))?;
    temporary.as_file().sync_all().map_err(|error| Error::io("Cannot sync configuration", error))?;
    temporary.persist(&config).map_err(|error| Error::io("Cannot replace configuration", error.error))?;
    Ok(true)
}
