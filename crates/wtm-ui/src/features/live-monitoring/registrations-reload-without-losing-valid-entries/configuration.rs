use crate::process::{self, args};
use crate::{Cancel, Error};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    repos: Vec<PathBuf>,
}

pub fn read(path: &Path) -> Result<Vec<u8>, Error> {
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

pub fn parse(bytes: &[u8]) -> Result<Vec<PathBuf>, Error> {
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

pub fn identity(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) { text.to_lowercase() } else { text.into_owned() }
}

pub fn resolve(path: &Path, cancel: &Cancel) -> Result<PathBuf, Error> {
    let output = process::checked(Some(path), &args(&["rev-parse", "--path-format=absolute", "--git-common-dir"]), cancel)?;
    let output = output.strip_suffix(b"\n").unwrap_or(&output);
    let output = output.strip_suffix(b"\r").unwrap_or(output);
    let common = crate::git::path(output)?;
    dunce::canonicalize(&common).map_err(|error| Error::io(format!("Cannot resolve {}", common.display()), error))
}

pub fn encode(repos: Vec<PathBuf>) -> Result<Vec<u8>, Error> {
    serde_json::to_vec_pretty(&Registry { repos })
        .map_err(|error| Error::config(format!("Cannot encode configuration: {error}")))
}
