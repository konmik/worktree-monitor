use crate::order::identity;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub const VERSION: u32 = 1;
pub const MAX_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VisibilityKey {
    pub repository: PathBuf,
    pub worktree: PathBuf,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Visibility {
    hidden: Vec<VisibilityKey>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredVisibility {
    version: u32,
    hidden: Vec<VisibilityKey>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn key(repository: &Path, worktree: &Path) -> io::Result<VisibilityKey> {
    if !repository.is_absolute() || !worktree.is_absolute() {
        return Err(invalid("Visibility paths must be absolute"));
    }
    Ok(VisibilityKey { repository: identity(repository), worktree: identity(worktree) })
}

impl Visibility {
    fn parse(bytes: &[u8]) -> io::Result<Self> {
        let mut stored: StoredVisibility = serde_json::from_slice(bytes)
            .map_err(|error| invalid(format!("Invalid worktree visibility: {error}")))?;
        if stored.version != VERSION {
            return Err(invalid("Unsupported worktree visibility version"));
        }
        let mut seen = HashSet::new();
        for entry in &mut stored.hidden {
            let normalized = key(&entry.repository, &entry.worktree)?;
            if !seen.insert(normalized.clone()) {
                return Err(invalid("Duplicate hidden worktree"));
            }
            *entry = normalized;
        }
        Ok(Self { hidden: stored.hidden })
    }

    pub fn read(path: &Path) -> io::Result<Self> {
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes)?;
        if bytes.len() > MAX_BYTES {
            return Err(invalid("Worktree visibility exceeds 1 MiB"));
        }
        Self::parse(&bytes)
    }

    pub fn hidden(&self) -> &[VisibilityKey] {
        &self.hidden
    }

    pub fn contains(&self, repository: &Path, worktree: &Path) -> bool {
        key(repository, worktree).ok().is_some_and(|key| self.hidden.contains(&key))
    }

    pub fn persist_toggle(path: &Path, repository: &Path, worktree: &Path) -> io::Result<Self> {
        let target = key(repository, worktree)?;
        let parent = path.parent().ok_or_else(|| invalid("Worktree visibility has no parent"))?;
        let parent = if parent.as_os_str().is_empty() { Path::new(".") } else { parent };
        fs::create_dir_all(parent)?;
        let lock = OpenOptions::new().create(true).truncate(false).read(true).write(true)
            .open(path.with_extension("lock"))?;
        lock.try_lock_exclusive().map_err(|error| io::Error::new(error.kind(),
            format!("Cannot lock worktree visibility: {error}")))?;
        let latest = Self::read(path)?;
        let mut next = latest.clone();
        if let Some(index) = next.hidden.iter().position(|entry| entry == &target) {
            next.hidden.remove(index);
        } else {
            next.hidden.push(target);
        }
        let bytes = serde_json::to_vec_pretty(&StoredVisibility { version: VERSION, hidden: next.hidden.clone() })?;
        if bytes.len() > MAX_BYTES {
            return Err(invalid("Worktree visibility exceeds 1 MiB"));
        }
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|error| error.error)?;
        Ok(next)
    }
}
