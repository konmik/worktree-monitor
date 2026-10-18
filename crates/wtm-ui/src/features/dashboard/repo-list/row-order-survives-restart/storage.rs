use super::{Order, OrderMove, WorktreeOrder, identity, invalid};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use crate::Snapshot;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredOrder {
    version: u32,
    repositories: Vec<PathBuf>,
    worktrees: Vec<WorktreeOrder>,
}

fn normalize(paths: &mut Vec<PathBuf>) -> io::Result<()> {
    if paths.iter().any(|path| !path.is_absolute()) { return Err(invalid("Order paths must be absolute")); }
    let mut seen = HashSet::new();
    for path in paths.iter_mut() { *path = identity(path); }
    paths.retain(|path| seen.insert(path.clone()));
    Ok(())
}

impl Order {
    fn parse(bytes: &[u8]) -> io::Result<Self> {
        let mut stored: StoredOrder = serde_json::from_slice(bytes).map_err(|error| invalid(format!("Invalid presentation order: {error}")))?;
        if stored.version != 1 { return Err(invalid("Unsupported presentation order version")); }
        normalize(&mut stored.repositories)?;
        let mut seen = HashSet::new();
        for section in &mut stored.worktrees {
            if !section.repository.is_absolute() { return Err(invalid("Order repository must be absolute")); }
            section.repository = identity(&section.repository);
            if !seen.insert(section.repository.clone()) { return Err(invalid("Duplicate worktree order section")); }
            normalize(&mut section.paths)?;
        }
        Ok(Self { repositories: stored.repositories, worktrees: stored.worktrees })
    }

    pub fn read(path: &Path) -> io::Result<Self> {
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 { return Err(invalid("Presentation order exceeds 1 MiB")); }
        Self::parse(&bytes)
    }

    pub fn persist_move(path: &Path, snapshot: &Snapshot, movement: &OrderMove) -> io::Result<(Self, bool)> {
        let parent = path.parent().ok_or_else(|| invalid("Presentation order has no parent"))?;
        let parent = if parent.as_os_str().is_empty() { Path::new(".") } else { parent };
        fs::create_dir_all(parent)?;
        let lock = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(path.with_extension("lock"))?;
        lock.try_lock_exclusive().map_err(|error| io::Error::new(error.kind(), format!("Cannot lock presentation order: {error}")))?;
        let latest = Self::read(path)?;
        let Some(next) = latest.moved(snapshot, movement)? else { return Ok((latest, false)); };
        let bytes = serde_json::to_vec_pretty(&StoredOrder { version: 1, repositories: next.repositories.clone(), worktrees: next.worktrees.clone() })?;
        if bytes.len() > 1024 * 1024 { return Err(invalid("Presentation order exceeds 1 MiB")); }
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|error| error.error)?;
        Ok((next, true))
    }
}
