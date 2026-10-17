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
