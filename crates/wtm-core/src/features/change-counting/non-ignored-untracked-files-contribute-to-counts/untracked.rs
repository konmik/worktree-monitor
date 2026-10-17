use crate::process::{self, args};
use crate::git::{invalid, relative_path};
use crate::{Cancel, Counts, Error, ErrorKind};
use super::classification::numstat;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

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

pub(super) fn count(tree: &Path, cancel: &Cancel) -> Result<Counts, Error> {
    let mut counts = Counts::default();
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
    Ok(counts)
}
