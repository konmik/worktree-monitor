#[test]
fn branch_changes_include_commits_and_net_working_tree_changes() {
    for format in ["sha1", "sha256"] {
        let fixture = Fixture::new(format);
        fixture.write("lines.txt", "base\n");
        fixture.commit();
        git(&fixture.repo, &["checkout", "--quiet", "-b", "feature"]);
        fixture.write("lines.txt", "base\ncommitted\n");
        fixture.commit();
        fixture.service().add(&fixture.repo).unwrap();
        assert_eq!(fixture.stats().counts, source_counts(1, 0, 0));
        fixture.write("lines.txt", "base\ncommitted\nstaged\n");
        git(&fixture.repo, &["add", "lines.txt"]);
        fixture.write("lines.txt", "base\ncommitted\nstaged\nunstaged\n");
        fixture.write("new.txt", "untracked\n");
        assert_eq!(fixture.stats().counts, source_counts(4, 0, 0));
        fixture.write("lines.txt", "base\n");
        fs::remove_file(fixture.repo.join("new.txt")).unwrap();
        assert_eq!(fixture.stats().counts, Counts::default());
    }
}

#[test]
fn branch_changes_use_main_then_master_and_fall_back_without_either() {
    let fixture = Fixture::new("sha1");
    fixture.write("lines.txt", "base\n");
    fixture.commit();
    git(&fixture.repo, &["branch", "-m", "master"]);
    git(&fixture.repo, &["checkout", "--quiet", "-b", "feature"]);
    fixture.write("lines.txt", "base\nfeature\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    assert_eq!(fixture.stats().counts, source_counts(1, 0, 0));
    git(&fixture.repo, &["branch", "main"]);
    assert_eq!(fixture.stats().counts, Counts::default());
    git(&fixture.repo, &["branch", "-D", "main", "master"]);
    assert_eq!(fixture.stats().counts, Counts::default());
    fixture.write("lines.txt", "base\nfeature\nlocal\n");
    assert_eq!(fixture.stats().counts, source_counts(1, 0, 0));
}

#[test]
fn branch_changes_exclude_diverged_main_commits_and_support_detached_head() {
    let fixture = Fixture::new("sha1");
    fixture.write("lines.txt", "base\n");
    fixture.commit();
    git(&fixture.repo, &["checkout", "--quiet", "-b", "feature"]);
    fixture.write("lines.txt", "base\nfeature\n");
    fixture.commit();
    git(&fixture.repo, &["checkout", "--quiet", "main"]);
    fixture.write("upstream.txt", "main only\n");
    fixture.commit();
    git(&fixture.repo, &["checkout", "--quiet", "--detach", "feature"]);
    fixture.service().add(&fixture.repo).unwrap();
    let stats = fixture.stats();
    assert!(matches!(stats.head, Head::Detached(_)));
    assert_eq!(stats.counts, source_counts(1, 0, 0));
}

#[test]
fn branch_changes_fall_back_to_head_for_unrelated_main() {
    let fixture = Fixture::new("sha1");
    fixture.write("lines.txt", "original\n");
    fixture.commit();
    git(&fixture.repo, &["checkout", "--quiet", "--orphan", "feature"]);
    fixture.write("lines.txt", "unrelated\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    assert_eq!(fixture.stats().counts, Counts::default());
    fixture.write("lines.txt", "unrelated\nlocal\n");
    assert_eq!(fixture.stats().counts, source_counts(1, 0, 0));
}

#[test]
fn branch_changes_keep_unborn_empty_tree_with_existing_main() {
    let fixture = Fixture::new("sha1");
    fixture.write("lines.txt", "main\n");
    fixture.commit();
    git(&fixture.repo, &["checkout", "--quiet", "--orphan", "feature"]);
    fixture.write("lines.txt", "new root\n");
    fixture.service().add(&fixture.repo).unwrap();
    let stats = fixture.stats();
    assert_eq!(stats.head, Head::Unborn("feature".into()));
    assert_eq!(stats.counts, source_counts(1, 0, 0));
}

#[test]
fn net_staged_unstaged_reverted_stage_and_untracked_lines() {
    let fixture = Fixture::new("sha1");
    fixture.write("lines.txt", "one\ntwo\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
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
fn categorized_staged_unstaged_untracked_rename_and_binary_changes_use_destination_paths() {
    let fixture = Fixture::new("sha1");
    fs::create_dir_all(fixture.repo.join("tests/tools")).unwrap();
    fs::create_dir(fixture.repo.join("scripts")).unwrap();
    fixture.write("source.txt", "one\ntwo\n");
    fixture.write("tests/behavior.test.js", "old\n");
    fixture.write("old-source.txt", "keep\nrename\nsame1\nsame2\nsame3\nsame4\nsame5\nsame6\nsame7\nsame8\n");
    fixture.write("asset.bin", b"old\0data");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    fixture.write("source.txt", "one\ntwo\nthree\n");
    git(&fixture.repo, &["add", "source.txt"]);
    fixture.write("tests/behavior.test.js", "old\nnew\n");
    git(&fixture.repo, &["mv", "old-source.txt", "tests/renamed.test.js"]);
    fixture.write("tests/renamed.test.js", "keep\nchanged\nsame1\nsame2\nsame3\nsame4\nsame5\nsame6\nsame7\nsame8\n");
    fixture.write("scripts/new.ps1", "one\ntwo\n");
    fixture.write("tests/tools/fixture.ps1", "one\n");
    fixture.write("asset.bin", b"new\0data");
    assert_eq!(fixture.stats().counts, Counts {
        source: LineCounts { additions: 1, deletions: 0 },
        test: LineCounts { additions: 3, deletions: 1 },
        extra: LineCounts { additions: 2, deletions: 0 },
        binaries: 1,
    });
}

#[test]
fn attributes_ignored_and_binary_files_use_git_semantics() {
    let fixture = Fixture::new("sha1");
    fixture.write(".gitattributes", "forced.txt binary\n");
    fixture.write(".gitignore", "ignored.txt\n");
    fixture.write("tracked.bin", b"old\0data");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    fixture.write("forced.txt", "looks like text\n");
    fixture.write("ignored.txt", "must not count\n");
    fixture.write("tracked.bin", b"new\0data");
    assert_eq!(fixture.stats().counts, source_counts(0, 0, 2));
}

#[test]
fn unborn_sha1_and_sha256_with_staged_and_untracked_files() {
    for format in ["sha1", "sha256"] {
        let fixture = Fixture::new(format);
        fixture.write("staged.txt", "first\n");
        git(&fixture.repo, &["add", "staged.txt"]);
        fixture.write("staged.txt", "first\nsecond\n");
        fixture.write("new.txt", "last");
        fixture.service().add(&fixture.repo).unwrap();
        let stats = fixture.stats();
        assert_eq!(stats.head, Head::Unborn("main".into()));
        assert_eq!(stats.counts, source_counts(3, 0, 0));
        assert_eq!(git(&fixture.repo, &["rev-list", "--all"]), b"");
    }
}

#[test]
fn indexed_rename_is_zero_and_detached_head_is_visible() {
    let fixture = Fixture::new("sha1");
    fixture.write("old 树.txt", "same\ncontent\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    git(&fixture.repo, &["mv", "old 树.txt", "new 树.txt"]);
    assert_eq!(fixture.stats().counts, source_counts(0, 0, 0));
    git(&fixture.repo, &["checkout", "--quiet", "--detach"]);
    let oid = String::from_utf8(git(&fixture.repo, &["rev-parse", "HEAD"])).unwrap().trim().to_owned();
    assert_eq!(fixture.stats().head, Head::Detached(oid));
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
    fixture.service().add(&fixture.repo).unwrap();
    let snapshot = fixture.service().snapshot(&Cancel::new());
    let trees = snapshot.repos[0].worktrees.as_ref().unwrap();
    assert_eq!(trees[0].result.as_ref().unwrap_err().kind, ErrorKind::Conflict);
    assert!(snapshot.text().contains("Unresolved merge conflicts"));
    assert!(!snapshot.text().contains("S +0/-0"));
}

#[test]
fn nested_untracked_repository_is_not_silently_skipped() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "baseline\n");
    fixture.commit();
    let nested = fixture.repo.join("nested");
    fs::create_dir(&nested).unwrap();
    git(&nested, &["init", "--quiet"]);
    fs::write(nested.join("new.txt"), "nested\n").unwrap();
    fixture.service().add(&fixture.repo).unwrap();
    let snapshot = fixture.service().snapshot(&Cancel::new());
    let trees = snapshot.repos[0].worktrees.as_ref().unwrap();
    assert_eq!(trees[0].result.as_ref().unwrap_err().kind, ErrorKind::Io);
    assert!(snapshot.text().contains("Unsupported untracked entry"));
}

#[test]
fn scans_do_not_write_the_index_config_or_git_objects() {
    let fixture = Fixture::new("sha256");
    fixture.write("staged.txt", "staged\n");
    git(&fixture.repo, &["add", "staged.txt"]);
    fixture.service().add(&fixture.repo).unwrap();
    fixture.write("untracked-one.txt", "one\n");
    fixture.write("untracked-two.txt", "two\n");
    let index = fs::read(fixture.repo.join(".git/index")).unwrap();
    let config = fs::read(&fixture.config).unwrap();
    let before_objects = git(&fixture.repo, &["count-objects", "-v"]);
    assert_eq!(fixture.stats().counts, source_counts(3, 0, 0));
    assert_eq!(fs::read(fixture.repo.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(&fixture.config).unwrap(), config);
    assert_eq!(git(&fixture.repo, &["count-objects", "-v"]), before_objects);
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
    fixture.service().add(&fixture.repo).unwrap();
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
    fixture.service().add(&fixture.repo).unwrap();
    fixture.write("locked.txt", "changed\nnew\n");
    let locked = fs::OpenOptions::new().read(true).share_mode(0).open(fixture.repo.join("locked.txt")).unwrap();
    let snapshot = fixture.service().snapshot(&Cancel::new());
    assert!(snapshot.has_errors(), "Unexpected successful scan: {}", snapshot.text());
    assert!(snapshot.repos[0].worktrees.as_ref().unwrap()[0].result.is_err());
    drop(locked);
    assert_eq!(fixture.stats().counts, source_counts(2, 1, 0));
}

#[cfg(windows)]
#[test]
fn unreadable_untracked_file_is_an_error_not_a_partial_count() {
    use std::os::windows::fs::OpenOptionsExt;
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "baseline\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    fixture.write("locked.txt", "new\n");
    fixture.write("readable.txt", "readable\n");
    let locked = fs::OpenOptions::new().read(true).share_mode(0).open(fixture.repo.join("locked.txt")).unwrap();
    let snapshot = fixture.service().snapshot(&Cancel::new());
    assert!(snapshot.has_errors(), "Unexpected successful scan: {}", snapshot.text());
    assert!(snapshot.repos[0].worktrees.as_ref().unwrap()[0].result.is_err());
    drop(locked);
    assert_eq!(fixture.stats().counts, source_counts(2, 0, 0));
}

#[test]
fn untracked_batches_preserve_literal_paths_and_git_attributes() {
    let fixture = Fixture::new("sha1");
    fixture.write(".gitattributes", "*.bin binary\n*.txt diff\n");
    fixture.write(".gitignore", "ignored.txt\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    fs::create_dir(fixture.repo.join("tests")).unwrap();
    fs::create_dir(fixture.repo.join("scripts")).unwrap();
    fixture.write("literal [x] 树.txt", "one\ntwo\n");
    fixture.write("forced-text.txt", b"one\0two\n");
    fixture.write("tests/new.test.js", "one\ntwo\n");
    fixture.write("scripts/tool.ps1", "one\n");
    fixture.write("asset.bin", "forced binary\n");
    fixture.write("ignored.txt", "not counted\n");
    assert_eq!(fixture.stats().counts, Counts {
        source: LineCounts { additions: 3, deletions: 0 },
        test: LineCounts { additions: 2, deletions: 0 },
        extra: LineCounts { additions: 1, deletions: 0 },
        binaries: 1,
    });
}

#[test]
fn untracked_batches_cover_multiple_command_chunks() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    for index in 0..250 {
        fixture.write(&format!("untracked-{index:03}-with-spaces-[literal]-and-a-long-name-to-fill-a-command.txt"), "one\n");
    }
    assert_eq!(fixture.stats().counts, source_counts(250, 0, 0));
}

#[test]
fn branch_changes_monitor_refreshes_when_main_advances_without_head_moving() {
    let fixture = Fixture::new("sha1");
    fixture.write("lines.txt", "base\n");
    fixture.commit();
    git(&fixture.repo, &["checkout", "--quiet", "-b", "feature"]);
    fixture.write("lines.txt", "base\nfeature\n");
    fixture.commit();
    let head = git(&fixture.repo, &["rev-parse", "HEAD"]);
    fixture.service().add(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees|
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == source_counts(1, 0, 0))));
    git(&fixture.repo, &["update-ref", "refs/heads/main", "HEAD"]);
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == Counts::default(), Duration::from_secs(18));
    assert_eq!(git(&fixture.repo, &["rev-parse", "HEAD"]), head);
    monitor.stop().unwrap();
}
