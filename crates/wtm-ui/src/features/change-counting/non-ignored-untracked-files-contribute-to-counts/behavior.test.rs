#[test]
fn attributes_ignored_and_binary_files_use_git_semantics() {
    let fixture = Fixture::new("sha1");
    fixture.write(".gitattributes", "forced.txt binary\n");
    fixture.write(".gitignore", "ignored.txt\n");
    fixture.write("tracked.bin", b"old\0data");
    fixture.commit();
    fixture.register(&fixture.repo).unwrap();
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
        fixture.register(&fixture.repo).unwrap();
        let stats = fixture.stats();
        assert_eq!(stats.head, Head::Unborn("main".into()));
        assert_eq!(stats.counts, source_counts(3, 0, 0));
        assert_eq!(git(&fixture.repo, &["rev-list", "--all"]), b"");
    }
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
    fixture.register(&fixture.repo).unwrap();
    let snapshot = fixture.service().snapshot(&Cancel::new());
    let trees = snapshot.repos[0].worktrees.as_ref().unwrap();
    assert_eq!(trees[0].result.as_ref().unwrap_err().kind, ErrorKind::Io);
    assert!(snapshot.text().contains("Unsupported untracked entry"));
}

#[cfg(windows)]
#[test]
fn unreadable_untracked_file_is_an_error_not_a_partial_count() {
    use std::os::windows::fs::OpenOptionsExt;
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "baseline\n");
    fixture.commit();
    fixture.register(&fixture.repo).unwrap();
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
    fixture.register(&fixture.repo).unwrap();
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
    fixture.register(&fixture.repo).unwrap();
    for index in 0..250 {
        fixture.write(&format!("untracked-{index:03}-with-spaces-[literal]-and-a-long-name-to-fill-a-command.txt"), "one\n");
    }
    assert_eq!(fixture.stats().counts, source_counts(250, 0, 0));
}
