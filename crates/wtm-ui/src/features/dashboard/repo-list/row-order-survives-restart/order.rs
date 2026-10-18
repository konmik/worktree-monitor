use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use crate::{ScanTarget, Snapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrderMove {
    Repository { repository: PathBuf, before: Option<PathBuf> },
    Worktree { repository: PathBuf, worktree: PathBuf, before: Option<PathBuf> },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Order {
    repositories: Vec<PathBuf>,
    worktrees: Vec<WorktreeOrder>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct WorktreeOrder {
    repository: PathBuf,
    paths: Vec<PathBuf>,
}

pub fn identity(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy().replace('\\', "/").to_lowercase();
        let text = if let Some(unc) = text.strip_prefix("//?/unc/") { format!("//{unc}") }
            else { text.strip_prefix("//?/").unwrap_or(&text).to_owned() };
        PathBuf::from(text)
    }
    #[cfg(not(windows))]
    { path.to_path_buf() }
}

fn invalid(message: impl Into<String>) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, message.into()) }

impl Order {

    pub fn apply(&self, snapshot: &mut Snapshot) {
        snapshot.repos.sort_by_key(|repo| rank(&self.repositories, &repo.common_dir));
        for repo in &mut snapshot.repos {
            if let (Some(section), Ok(trees)) = (self.worktrees.iter().find(|section| section.repository == identity(&repo.common_dir)), &mut repo.worktrees) {
                trees.sort_by_key(|tree| rank(&section.paths, &tree.path));
            }
        }
    }

    pub fn apply_targets(&self, targets: &mut [ScanTarget]) {
        let mut repositories: Vec<_> = targets.iter().filter_map(|target| match target {
            ScanTarget::Repository { common_dir, .. } => Some(identity(common_dir)),
            ScanTarget::Worktree { .. } => None,
        }).collect();
        repositories.sort_by_key(|repository| rank(&self.repositories, repository));
        targets.sort_by_key(|target| {
            let (repository, child, worktree_rank) = match target {
                ScanTarget::Repository { common_dir, .. } => (common_dir, false, 0),
                ScanTarget::Worktree { repository, path } => {
                    let section = self.worktrees.iter().find(|section| section.repository == identity(repository));
                    (repository, true, section.map_or(usize::MAX, |section| rank(&section.paths, path)))
                }
            };
            (rank(&repositories, repository), child, worktree_rank)
        });
    }

    pub fn moved(&self, snapshot: &Snapshot, movement: &OrderMove) -> io::Result<Option<Self>> {
        let mut result = self.clone();
        let (ranks, source, before, members) = match movement {
            OrderMove::Repository { repository, before } => (&mut result.repositories, repository, before,
                snapshot.repos.iter().map(|repo| identity(&repo.common_dir)).collect::<Vec<_>>()),
            OrderMove::Worktree { repository, worktree, before } => {
                let repo = snapshot.repos.iter().find(|repo| identity(&repo.common_dir) == identity(repository))
                    .ok_or_else(|| invalid("Repository is no longer available"))?;
                let trees = repo.worktrees.as_ref().map_err(|_| invalid("Repository worktrees are unavailable"))?;
                let key = identity(repository);
                let index = if let Some(index) = result.worktrees.iter().position(|section| section.repository == key) { index }
                    else { result.worktrees.push(WorktreeOrder { repository: key, paths: Vec::new() }); result.worktrees.len() - 1 };
                (&mut result.worktrees[index].paths, worktree, before, trees.iter().map(|tree| identity(&tree.path)).collect())
            }
        };
        let source = identity(source);
        let before = before.as_deref().map(identity);
        if !members.contains(&source) || before.as_ref().is_some_and(|path| !members.contains(path)) {
            return Err(invalid("Move must refer to existing members of the same group"));
        }
        if before.as_ref() == Some(&source) { return Ok(None); }
        let mut visible = members.clone();
        visible.sort_by_key(|path| rank(ranks, path));
        let original = visible.clone();
        relocate(&mut visible, &source, before.as_deref());
        if visible == original { return Ok(None); }
        for member in members {
            if !ranks.contains(&member) { ranks.push(member); }
        }
        relocate(ranks, &source, before.as_deref());
        Ok(Some(result))
    }

}

fn rank(paths: &[PathBuf], path: &Path) -> usize {
    let key = identity(path);
    paths.iter().position(|saved| *saved == key).unwrap_or(usize::MAX)
}

fn relocate(paths: &mut Vec<PathBuf>, source: &Path, before: Option<&Path>) {
    paths.retain(|path| path != source);
    let position = before.and_then(|before| paths.iter().position(|path| path == before)).unwrap_or(paths.len());
    paths.insert(position, source.to_path_buf());
}

#[path = "storage.rs"]
mod storage;
