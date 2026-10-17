#[test]
fn launch_context_includes_the_real_root_from_a_nested_directory_without_registration() {
    let fixture = Fixture::new("sha1");
    let nested = fixture.repo.join("nested/deeper");
    fs::create_dir_all(&nested).unwrap();
    fixture.write("new.txt", "first\nsecond\n");
    let mut monitor = fixture.service().monitor_from(nested).unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1);
    let snapshot = monitor.latest().unwrap().snapshot.clone();
    let repo = &snapshot.repos[0];
    assert_eq!(repo.common_dir, dunce::canonicalize(fixture.repo.join(".git")).unwrap());
    let trees = repo.worktrees.as_ref().unwrap();
    assert_eq!(trees.len(), 1);
    assert_eq!(trees[0].path, dunce::canonicalize(&fixture.repo).unwrap());
    assert_eq!(trees[0].result.as_ref().unwrap().head, Head::Unborn("main".into()));
    assert_eq!(trees[0].result.as_ref().unwrap().counts, source_counts(2, 0, 0));
    assert!(!fixture.config.exists());
    assert_eq!(fixture.service().snapshot(&Cancel::new()).text(), "No registered repositories.\n");
    let mut normal = fixture.service().monitor().unwrap();
    wait_for(&normal, |update| update.snapshot.repos.is_empty());
    normal.stop().unwrap();
    monitor.stop().unwrap();
    assert!(!fixture.config.exists());
}

#[test]
fn launch_from_a_linked_tree_discovers_siblings_and_deduplicates_registrations() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    let linked = fixture._directory.path().join("linked");
    git(&fixture.repo, &["worktree", "add", "--quiet", "--detach", linked.to_str().unwrap()]);
    let nested = linked.join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(linked.join("tracked.txt"), "before\nafter\n").unwrap();
    let mut unregistered = fixture.service().monitor_from(nested.clone()).unwrap();
    wait_for(&unregistered, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees| trees.len() == 2));
    let trees = unregistered.latest().unwrap().snapshot.repos[0].worktrees.clone().unwrap();
    assert_eq!(trees.len(), 2);
    let tree = trees.iter().find(|tree| tree.path == dunce::canonicalize(&linked).unwrap()).unwrap();
    assert!(matches!(tree.result.as_ref().unwrap().head, Head::Detached(_)));
    assert_eq!(tree.result.as_ref().unwrap().counts.source.additions, 1);
    assert!(!fixture.config.exists());
    unregistered.stop().unwrap();
    fixture.service().add(&fixture.repo).unwrap();
    let bytes = fs::read(&fixture.config).unwrap();
    let mut registered = fixture.service().monitor_from(nested).unwrap();
    wait_for(&registered, |update| update.snapshot.repos.len() == 1
        && update.snapshot.repos[0].worktrees.as_ref().is_ok_and(|trees| trees.len() == 2));
    assert_eq!(registered.latest().unwrap().snapshot.repos[0].worktrees.as_ref().unwrap().len(), 2);
    assert_eq!(fs::read(&fixture.config).unwrap(), bytes);
    registered.stop().unwrap();
}

#[test]
fn launch_context_appends_to_registered_order_and_remains_after_unregistration() {
    let first = Fixture::new("sha1");
    let current = Fixture::new("sha1");
    first.service().add(&first.repo).unwrap();
    let bytes = fs::read(&first.config).unwrap();
    let mut monitor = first.service().monitor_from(current.repo.clone()).unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 2);
    let snapshot = monitor.latest().unwrap().snapshot.clone();
    assert_eq!(snapshot.repos[0].common_dir, dunce::canonicalize(first.repo.join(".git")).unwrap());
    assert_eq!(snapshot.repos[1].common_dir, dunce::canonicalize(current.repo.join(".git")).unwrap());
    assert_eq!(fs::read(&first.config).unwrap(), bytes);
    first.service().add(&current.repo).unwrap();
    let registered = fs::read(&first.config).unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 2 && update.snapshot.config_error.is_none());
    assert_eq!(fs::read(&first.config).unwrap(), registered);
    first.service().remove(&current.repo).unwrap();
    first.service().remove(&first.repo).unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1);
    assert_eq!(monitor.latest().unwrap().snapshot.repos[0].common_dir, dunce::canonicalize(current.repo.join(".git")).unwrap());
    monitor.stop().unwrap();
}

#[test]
fn launch_outside_git_adds_nothing_and_bare_launch_does_not_fabricate_a_worktree() {
    let fixture = Fixture::new("sha1");
    let mut outside = fixture.service().monitor_from(fixture._directory.path().to_path_buf()).unwrap();
    wait_for(&outside, |update| update.snapshot.repos.is_empty());
    assert_eq!(outside.latest().unwrap().launch_error, None);
    outside.stop().unwrap();
    let bare = fixture._directory.path().join("bare.git");
    fs::create_dir(&bare).unwrap();
    git(&bare, &["init", "--bare", "--quiet"]);
    let mut monitor = fixture.service().monitor_from(bare.clone()).unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1);
    let snapshot = monitor.latest().unwrap().snapshot.clone();
    assert_eq!(snapshot.repos[0].common_dir, dunce::canonicalize(bare).unwrap());
    assert_eq!(snapshot.repos[0].worktrees, Ok(Vec::new()));
    assert!(!snapshot.has_errors());
    monitor.stop().unwrap();
    assert!(!fixture.config.exists());
}

#[test]
fn existing_non_git_launch_discovers_a_later_git_init() {
    let fixture = Fixture::new("sha1");
    let launch = fixture._directory.path().join("launch");
    fs::create_dir(&launch).unwrap();
    let mut monitor = fixture.service().monitor_from(launch.clone()).unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.is_empty() && update.launch_error.is_none());
    thread::sleep(Duration::from_millis(100));
    git(&launch, &["init", "--quiet", "--initial-branch=main"]);
    let common = dunce::canonicalize(launch.join(".git")).unwrap();
    let elapsed = wait_for_within(&monitor, |update| {
        let Some(repo) = update.snapshot.repos.first() else { return false; };
        let Some(trees) = repo.worktrees.as_ref().ok() else { return false; };
        trees.len() == 1 && repo.common_dir == common
            && trees[0].result.as_ref().is_ok_and(|stats| {
                stats.head == Head::Unborn("main".into()) && stats.counts == Counts::default()
            })
    }, Duration::from_secs(18));
    assert!(elapsed < Duration::from_secs(18), "git init took {elapsed:?}");
    monitor.stop().unwrap();
}

#[test]
fn missing_launch_directory_reports_an_error_and_valid_retry_clears_it() {
    let fixture = Fixture::new("sha1");
    let launch = fixture._directory.path().join("missing");
    let mut monitor = fixture.service().monitor_from(launch.clone()).unwrap();
    wait_for(&monitor, |update| update.launch_error.is_some());
    let failed = monitor.latest().unwrap();
    let error = failed.launch_error.as_ref().unwrap();
    assert_eq!(error.kind, ErrorKind::Git);
    assert!(error.message.contains("cannot change to"), "{}", error.message);
    assert!(failed.snapshot.repos.is_empty());
    assert_eq!(failed.snapshot.config_error, None);
    assert!(!fixture.config.exists());
    fs::rename(&fixture.repo, &launch).unwrap();
    wait_for(&monitor, |update| update.launch_error.is_none() && update.snapshot.repos.len() == 1);
    assert_eq!(monitor.latest().unwrap().snapshot.repos[0].common_dir, dunce::canonicalize(launch.join(".git")).unwrap());
    monitor.stop().unwrap();
    assert!(!fixture.config.exists());
}

#[test]
fn launch_error_clears_when_retry_is_outside_git() {
    let fixture = Fixture::new("sha1");
    let launch = fixture._directory.path().join("missing");
    let mut monitor = fixture.service().monitor_from(launch.clone()).unwrap();
    wait_for(&monitor, |update| update.launch_error.is_some());
    fs::create_dir(&launch).unwrap();
    wait_for(&monitor, |update| update.launch_error.is_none() && update.snapshot.repos.is_empty());
    assert_eq!(monitor.latest().unwrap().snapshot.config_error, None);
    monitor.stop().unwrap();
    assert!(!fixture.config.exists());
}

#[test]
fn malformed_registration_does_not_hide_or_register_a_valid_launch_repository() {
    let fixture = Fixture::new("sha1");
    fs::create_dir_all(fixture.config.parent().unwrap()).unwrap();
    fs::write(&fixture.config, b"broken").unwrap();
    let mut monitor = fixture.service().monitor_from(fixture.repo.clone()).unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1 && update.snapshot.config_error.is_some());
    assert_eq!(monitor.latest().unwrap().snapshot.repos[0].worktrees.as_ref().unwrap().len(), 1);
    assert_eq!(fs::read(&fixture.config).unwrap(), b"broken");
    monitor.stop().unwrap();
}

#[test]
fn previously_resolved_launch_repository_stays_visible_as_an_error_when_missing() {
    let fixture = Fixture::new("sha1");
    let mut monitor = fixture.service().monitor_from(fixture.repo.clone()).unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1);
    let common = monitor.latest().unwrap().snapshot.repos[0].common_dir.clone();
    fs::rename(&fixture.repo, fixture.repo.with_file_name("moved")).unwrap();
    fs::create_dir_all(fixture.config.parent().unwrap()).unwrap();
    fs::write(&fixture.config, b"{\"repos\":[]}").unwrap();
    wait_for(&monitor, |update| update.launch_error.is_some() && update.snapshot.repos.len() == 1 && update.snapshot.repos[0].worktrees.is_err());
    assert_eq!(monitor.latest().unwrap().snapshot.repos[0].common_dir, common);
    assert_eq!(fs::read(&fixture.config).unwrap(), b"{\"repos\":[]}");
    monitor.stop().unwrap();
}

#[cfg(windows)]
#[test]
fn launch_repository_case_alias_does_not_duplicate_a_registered_group() {
    let fixture = Fixture::new("sha1");
    fixture.service().add(&fixture.repo).unwrap();
    let bytes = fs::read(&fixture.config).unwrap();
    let alias = PathBuf::from(fixture.repo.to_string_lossy().to_uppercase());
    let mut monitor = fixture.service().monitor_from(alias).unwrap();
    wait_for(&monitor, |update| update.snapshot.repos.len() == 1);
    assert_eq!(monitor.latest().unwrap().snapshot.repos[0].worktrees.as_ref().unwrap().len(), 1);
    assert_eq!(fs::read(&fixture.config).unwrap(), bytes);
    monitor.stop().unwrap();
}
