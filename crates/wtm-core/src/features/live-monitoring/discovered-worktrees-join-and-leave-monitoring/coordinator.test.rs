use super::*;

#[test]
fn repository_discovery_does_not_refresh_existing_worktrees() {
    let (_directory, mut coordinator) = scan_fixture(2);
    coordinator.scan_pass();
    let common = coordinator.repo_order[0].clone();
    let previous = coordinator.repos[&common].activity.clone().unwrap();
    let trees: HashMap<_, _> = coordinator.worktrees.iter().map(|(key, state)| (key.clone(), state.activity.clone())).collect();
    let other = coordinator.repos[&coordinator.repo_order[1]].activity.clone();
    coordinator.discoveries.clear();
    coordinator.passes.changed.clear();
    coordinator.discoveries.insert(common.clone());
    let published = coordinator.published.len();
    coordinator.scan_pass();
    assert_eq!(coordinator.published.len() - published, 2);
    let start = &coordinator.published[published];
    let active: Vec<_> = start.scan_activities.iter().filter(|activity| activity.finished_at.is_none()).collect();
    assert_eq!(active.len(), 1);
    assert!(matches!(&active[0].target, ScanTarget::Repository { common_dir, .. } if normalize(common_dir) == common));
    assert!(active[0].generation > previous.generation);
    assert!(coordinator.repos[&common].activity.as_ref().unwrap().finished_at.is_some());
    assert_eq!(coordinator.repos[&coordinator.repo_order[1]].activity, other);
    for (key, activity) in trees { assert_eq!(coordinator.worktrees[&key].activity, activity); }
}

#[test]
fn discovery_schedules_new_worktrees_once_without_file_events() {
    let directory = tempfile::tempdir().unwrap();
    crate::process::checked(Some(directory.path()), &crate::process::args(&["init", "--quiet"]), &Cancel::new()).unwrap();
    let common_dir = directory.path().join(".git");
    let common = normalize(&common_dir);
    let latest = Arc::new(Mutex::new(None));
    let mut coordinator = Coordinator::new(Service::at("config.json"), None, Cancel::new(), latest);
    coordinator.repos.insert(common.clone(), RepoState {
        common_dir, path: directory.path().to_path_buf(), discovered: None,
        worktrees: Vec::new(), sources: Vec::new(),
        activity: None,
    });
    let mut discovered = git::DiscoveredRepo {
        path: directory.path().to_path_buf(),
        trees: vec![git::DiscoveredTree { path: directory.path().to_path_buf(), error: None }],
    };
    coordinator.apply_discovery(&common, Ok(discovered.clone()));
    assert_eq!(coordinator.passes.changed.len(), 1);
    coordinator.passes.changed.clear();
    coordinator.apply_discovery(&common, Ok(discovered.clone()));
    assert!(coordinator.passes.changed.is_empty());
    let new_path = directory.path().join("new-worktree");
    discovered.trees.push(git::DiscoveredTree { path: new_path.clone(), error: None });
    coordinator.apply_discovery(&common, Ok(discovered.clone()));
    assert_eq!(coordinator.passes.changed, HashSet::from([WorktreeKey::new(&common, &new_path)]));
    coordinator.passes.changed.clear();
    coordinator.apply_discovery(&common, Ok(discovered));
    assert!(coordinator.passes.changed.is_empty());
}
