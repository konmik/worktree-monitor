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
    fixture.register(&fixture.repo).unwrap();
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
