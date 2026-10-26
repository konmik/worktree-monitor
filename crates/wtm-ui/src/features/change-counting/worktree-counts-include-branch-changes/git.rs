use crate::process::{self, args};
use crate::{Cancel, Error, ErrorKind, Head, WorktreeStats};
use std::path::Path;
use crate::git::invalid;
#[cfg(test)]
pub(crate) use classification::numstat;
#[cfg(test)]
pub(crate) use classification::{category, Category};
#[path = "../changes-are-grouped-as-source-tests-tooling-and-binaries/classification.rs"]
mod classification;
#[path = "../non-ignored-untracked-files-contribute-to-counts/untracked.rs"]
mod untracked;

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
    remote_tips: Vec<String>,
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
    let remote_tips = crate::commit_indicators::remote_tips(tree, cancel)?;
    Ok(ScanIdentity { head, commit, base_ref, remote_tips })
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

#[path = "../worktree-scans-return-complete-counts-or-an-error/scan.rs"]
mod worktree_scan;
use worktree_scan::count_tree;

pub(crate) fn scan_tree(tree: &Path, cancel: &Cancel) -> Result<WorktreeStats, Error> {
    for _ in 0..2 {
        let identity = scan_identity(tree, cancel)?;
        let base = comparison_base(tree, &identity, cancel)?;
        let mut stats = count_tree(tree, identity.head.clone(), &base, cancel)?;
        if !matches!(identity.head, Head::Unborn(_)) {
            stats.pending_commits = crate::commit_indicators::pending(tree, &identity.commit,
                identity.base_ref.as_ref().map(|(_, oid)| oid.as_str()), &identity.remote_tips, cancel)?;
        }
        if scan_identity(tree, cancel)? == identity {
            return Ok(stats);
        }
    }
    Err(Error::new(ErrorKind::ChangedDuringScan, "HEAD or comparison references changed during the scan; retry on the next scan"))
}

