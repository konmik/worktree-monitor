#[test]
fn pending_commits_count_unpushed_or_unmerged_once() {
    for format in ["sha1", "sha256"] {
        let fixture = Fixture::new(format);
        fixture.write("base.txt", "base\n");
        fixture.commit();
        git(&fixture.repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        fixture.register(&fixture.repo).unwrap();
        assert_eq!(fixture.stats().pending_commits, 0);
        git(&fixture.repo, &["checkout", "--quiet", "-b", "feature"]);
        for index in 0..3 {
            fixture.write("feature.txt", format!("{index}\n"));
            fixture.commit();
        }
        assert_eq!(fixture.stats().pending_commits, 3);
        git(&fixture.repo, &["update-ref", "refs/remotes/origin/feature", "HEAD"]);
        assert_eq!(fixture.stats().pending_commits, 3);
        git(&fixture.repo, &["update-ref", "refs/heads/main", "HEAD"]);
        assert_eq!(fixture.stats().pending_commits, 0);
        git(&fixture.repo, &["update-ref", "-d", "refs/remotes/origin/feature"]);
        assert_eq!(fixture.stats().pending_commits, 3);
    }
}

#[test]
fn pending_commits_union_includes_distinct_pushed_and_merged_commits() {
    let fixture = Fixture::new("sha1");
    fixture.write("base.txt", "base\n");
    fixture.commit();
    git(&fixture.repo, &["branch", "feature"]);
    fixture.write("main.txt", "main\n");
    fixture.commit();
    git(&fixture.repo, &["checkout", "--quiet", "feature"]);
    fixture.write("feature.txt", "feature\n");
    fixture.commit();
    git(&fixture.repo, &["update-ref", "refs/remotes/origin/feature", "HEAD"]);
    git(&fixture.repo, &["-c", "user.name=Wtm Test", "-c", "user.email=wtm@example.invalid",
        "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", "merge", "--quiet", "--no-ff", "main", "-m", "Merge"]);
    fixture.register(&fixture.repo).unwrap();
    assert_eq!(fixture.stats().pending_commits, 3);
}

#[test]
fn pending_commits_support_unborn_detached_and_missing_comparison_branches() {
    let fixture = Fixture::new("sha1");
    fixture.register(&fixture.repo).unwrap();
    assert_eq!(fixture.stats().pending_commits, 0);
    fixture.write("base.txt", "base\n");
    fixture.commit();
    assert_eq!(fixture.stats().pending_commits, 1);
    git(&fixture.repo, &["checkout", "--quiet", "--detach"]);
    git(&fixture.repo, &["branch", "-D", "main"]);
    assert_eq!(fixture.stats().pending_commits, 1);
    git(&fixture.repo, &["update-ref", "refs/remotes/other/published", "HEAD"]);
    assert_eq!(fixture.stats().pending_commits, 0);
}

#[test]
fn pending_commits_monitor_refreshes_when_remote_references_change() {
    let fixture = Fixture::new("sha1");
    fixture.write("base.txt", "base\n");
    fixture.commit();
    fixture.register(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    let count_is = |update: &wtm_ui::MonitorUpdate, expected| update.snapshot.repos.first()
        .and_then(|repo| repo.worktrees.as_ref().ok()).and_then(|trees| trees.first())
        .is_some_and(|tree| tree.result.as_ref().is_ok_and(|stats| stats.pending_commits == expected));
    wait_for(&monitor, |update| count_is(update, 1));
    git(&fixture.repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    wait_for(&monitor, |update| count_is(update, 0));
    monitor.stop().unwrap();
}
