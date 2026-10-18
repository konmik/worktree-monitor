#[test]
fn net_staged_unstaged_reverted_stage_and_untracked_lines() {
    let fixture = Fixture::new("sha1");
    fixture.write("lines.txt", "one\ntwo\n");
    fixture.commit();
    fixture.register(&fixture.repo).unwrap();
    fixture.write("lines.txt", "one\ntwo\nstaged\n");
    git(&fixture.repo, &["add", "lines.txt"]);
    fixture.write("lines.txt", "one\ntwo\nstaged\nunstaged\n");
    assert_eq!(fixture.stats().counts, source_counts(2, 0, 0));
    fixture.write("lines.txt", "one\ntwo\n");
    assert_eq!(fixture.stats().counts, source_counts(0, 0, 0));
    fixture.write("untracked 树.txt", "first\nlast");
    assert_eq!(fixture.stats().counts, source_counts(2, 0, 0));
}

#[test]
fn conflicted_worktree_returns_error_without_valid_counts() {
    let fixture = Fixture::new("sha1");
    fixture.write("conflict.txt", "base\n");
    fixture.commit();
    git(&fixture.repo, &["checkout", "--quiet", "-b", "other"]);
    fixture.write("conflict.txt", "other\n");
    fixture.commit();
    git(&fixture.repo, &["checkout", "--quiet", "main"]);
    fixture.write("conflict.txt", "main\n");
    fixture.commit();
    let merge = Command::new("git").arg("-C").arg(&fixture.repo)
        .args(["-c", "user.name=Wtm Test", "-c", "user.email=wtm@example.invalid", "merge", "--no-edit", "other"])
        .output().unwrap();
    assert_eq!(merge.status.code(), Some(1));
    fixture.register(&fixture.repo).unwrap();
    let snapshot = fixture.service().snapshot(&Cancel::new());
    let trees = snapshot.repos[0].worktrees.as_ref().unwrap();
    assert_eq!(trees[0].result.as_ref().unwrap_err().kind, ErrorKind::Conflict);
    assert!(snapshot.text().contains("Unresolved merge conflicts"));
    assert!(!snapshot.text().contains("S +0/-0"));
}

#[test]
fn submodule_contents_are_not_counted_recursively() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "baseline\n");
    fixture.commit();
    let submodule = fixture.repo.join("submodule");
    fs::create_dir(&submodule).unwrap();
    git(&submodule, &["init", "--quiet"]);
    fs::write(submodule.join("nested.txt"), "baseline\n").unwrap();
    git(&submodule, &["add", "."]);
    git(&submodule, &["-c", "user.name=Wtm Test", "-c", "user.email=wtm@example.invalid", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", "commit", "--quiet", "-m", "Fixture"]);
    let oid = String::from_utf8(git(&submodule, &["rev-parse", "HEAD"])).unwrap();
    git(&fixture.repo, &["update-index", "--add", "--cacheinfo", &format!("160000,{},submodule", oid.trim())]);
    git(&fixture.repo, &["-c", "user.name=Wtm Test", "-c", "user.email=wtm@example.invalid", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", "commit", "--quiet", "-m", "Submodule fixture"]);
    fixture.register(&fixture.repo).unwrap();
    fs::write(submodule.join("nested.txt"), "edited\nadded\n").unwrap();
    fs::write(submodule.join("untracked.txt"), "untracked\n").unwrap();
    assert_eq!(fixture.stats().counts, source_counts(0, 0, 0));
}

#[cfg(windows)]
#[test]
fn unreadable_tracked_file_is_an_error_not_a_partial_count() {
    use std::os::windows::fs::OpenOptionsExt;
    let fixture = Fixture::new("sha1");
    fixture.write("locked.txt", "baseline\n");
    fixture.commit();
    fixture.register(&fixture.repo).unwrap();
    fixture.write("locked.txt", "changed\nnew\n");
    let locked = fs::OpenOptions::new().read(true).share_mode(0).open(fixture.repo.join("locked.txt")).unwrap();
    let snapshot = fixture.service().snapshot(&Cancel::new());
    assert!(snapshot.has_errors(), "Unexpected successful scan: {}", snapshot.text());
    assert!(snapshot.repos[0].worktrees.as_ref().unwrap()[0].result.is_err());
    drop(locked);
    assert_eq!(fixture.stats().counts, source_counts(2, 1, 0));
}
