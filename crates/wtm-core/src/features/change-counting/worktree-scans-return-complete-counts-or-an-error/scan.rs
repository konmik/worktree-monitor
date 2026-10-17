use crate::process::{self, args};
use crate::{Cancel, Error, ErrorKind, Head, WorktreeStats};
use super::{classification::numstat, untracked};
use std::path::Path;

pub(super) fn count_tree(tree: &Path, head: Head, base: &str, cancel: &Cancel) -> Result<WorktreeStats, Error> {
    let conflicts = process::checked(Some(tree), &args(&["ls-files", "--unmerged", "-z"]), cancel)?;
    if !conflicts.is_empty() { return Err(Error::new(ErrorKind::Conflict, "Unresolved merge conflicts")); }
    let mut command = args(&["diff", "--numstat", "-z", "--no-ext-diff", "--no-textconv", "--find-renames", "--ignore-submodules=all"]);
    command.push(base.into());
    command.push("--".into());
    let mut counts = numstat(&process::checked(Some(tree), &command, cancel)?)?;
    counts.add(untracked::count(tree, cancel)?)?;
    Ok(WorktreeStats { head, counts })
}
