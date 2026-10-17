use crate::{Counts, Error, LineCounts};
use crate::git::{invalid, path};
use std::path::Path;

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
