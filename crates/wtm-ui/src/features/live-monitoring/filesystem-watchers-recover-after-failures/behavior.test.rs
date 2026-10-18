#[test]
fn removed_registered_root_recovers_after_recreation() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    fixture.register(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && !update.snapshot.has_errors());
    let moved = fixture.repo.with_file_name("moved");
    fs::rename(&fixture.repo, &moved).unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1 && update.snapshot.has_errors());
    fs::rename(&moved, &fixture.repo).unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos.len() == 1
        && !update.snapshot.has_errors(), Duration::from_secs(18));
    monitor.stop().unwrap();
}

#[test]
fn replaced_refs_watch_observes_a_ref_only_head_and_count_change() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    git(&fixture.repo, &["switch", "--quiet", "-c", "alternate"]);
    fixture.write("tracked.txt", "alternate\n");
    fixture.commit();
    let alternate = String::from_utf8(git(&fixture.repo, &["rev-parse", "alternate"]))
        .unwrap().trim().to_owned();
    git(&fixture.repo, &["switch", "--quiet", "main"]);
    fixture.register(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees| {
            trees[0].result.as_ref().is_ok_and(|stats| {
                stats.head == Head::Branch("main".into()) && stats.counts == Counts::default()
            })
        }));
    let refs = fixture.repo.join(".git/refs");
    let moved = fixture.repo.join(".git/refs-away");
    fs::rename(&refs, &moved).unwrap();
    fs::rename(&moved, &refs).unwrap();
    git(&fixture.repo, &["update-ref", "refs/heads/main", &alternate]);
    let common = dunce::canonicalize(fixture.repo.join(".git")).unwrap();
    wait_for_within(&monitor, |update| {
        let Some(repo) = update.snapshot.repos.first() else { return false; };
        let Some(trees) = repo.worktrees.as_ref().ok() else { return false; };
        repo.common_dir == common && trees.len() == 1
            && trees[0].result.as_ref().is_ok_and(|stats| {
                stats.head == Head::Branch("main".into())
                    && stats.counts == source_counts(1, 1, 0)
            })
    }, Duration::from_secs(18));
    monitor.stop().unwrap();
}

#[test]
fn replaced_info_directory_reconnects_exclude_watch() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    fixture.write("ignored.txt", "untracked\n");
    fixture.register(&fixture.repo).unwrap();
    let mut monitor = fixture.service().monitor().unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees| {
            trees[0].result.as_ref().is_ok_and(|stats| stats.counts == source_counts(1, 0, 0))
        }));
    let info = fixture.repo.join(".git/info");
    let moved = fixture.repo.join(".git/info-away");
    fs::rename(&info, &moved).unwrap();
    fs::create_dir(&info).unwrap();
    fs::write(info.join("exclude"), b"ignored.txt\n").unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts == Counts::default(),
        Duration::from_secs(18));
    fs::write(info.join("exclude"), b"").unwrap();
    wait_for_within(&monitor, |update| update.snapshot.repos[0]
        .worktrees.as_ref().unwrap()[0].result.as_ref().unwrap().counts
        == source_counts(1, 0, 0), Duration::from_secs(18));
    monitor.stop().unwrap();
    fs::remove_dir_all(moved).unwrap();
}
