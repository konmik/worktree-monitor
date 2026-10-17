use crate::process::{self, args};
use crate::{Cancel, Counts, Error, ErrorKind, Head, LineCounts, WorktreeStats};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use super::{invalid, path, relative_path};

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

