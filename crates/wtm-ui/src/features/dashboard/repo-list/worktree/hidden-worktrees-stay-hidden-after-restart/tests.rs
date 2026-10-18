use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use wtm_ui::dashboard::{DashboardView, Health, ItemKey};
use wtm_ui::order::{identity, Order, OrderMove};
use wtm_ui::visibility::Visibility;
use wtm_core::{Counts, Error, ErrorKind, Head, RepoEntry, Snapshot, WorktreeEntry, WorktreeStats};

struct Fixture {
    directory: TempDir,
    snapshot: Snapshot,
}

impl Fixture {
    fn new() -> Self {
        let parent = std::env::temp_dir().join("opencode");
        fs::create_dir_all(&parent).unwrap();
        let directory = tempfile::tempdir_in(parent).unwrap();
        let repos = ["alpha", "beta"].into_iter().map(|name| {
            let path = directory.path().join(name);
            RepoEntry { common_dir: path.join(".git"), path: path.clone(), worktrees: Ok(vec![
                WorktreeEntry { path, result: Err(Error { kind: ErrorKind::Git, message: "Stats unavailable".into() }) },
                WorktreeEntry { path: directory.path().join(format!("{name}-tree")), result: Err(Error { kind: ErrorKind::Io, message: "Missing worktree".into() }) },
            ]) }
        }).collect();
        Self { directory, snapshot: Snapshot { repos, config_error: None } }
    }

    fn path(&self) -> PathBuf { self.directory.path().join("config.visibility.json") }

    fn first_tree(&self) -> (&Path, &Path) {
        let repo = &self.snapshot.repos[0];
        (&repo.common_dir, &repo.worktrees.as_ref().unwrap()[0].path)
    }
}

fn worktree_keys(view: &DashboardView) -> Vec<&ItemKey> {
    view.items.iter().filter_map(|item| match &item.key {
        ItemKey::Worktree(_, _) => Some(&item.key),
        _ => None,
    }).collect()
}

#[test]
fn hidden_identity_is_saved_and_restored() {
    let fixture = Fixture::new();
    let (repository, worktree) = fixture.first_tree();
    let visibility = Visibility::persist_toggle(&fixture.path(), repository, worktree).unwrap();
    assert!(visibility.contains(repository, worktree));
    let restored = Visibility::read(&fixture.path()).unwrap();
    assert_eq!(restored, visibility);
    assert_eq!(restored.hidden().len(), 1);
    assert_eq!(restored.hidden()[0].repository, identity(repository));
    assert_eq!(restored.hidden()[0].worktree, identity(worktree));
    let stored: serde_json::Value = serde_json::from_slice(&fs::read(fixture.path()).unwrap()).unwrap();
    assert_eq!(stored["version"], 1);
    assert_eq!(stored["hidden"].as_array().unwrap().len(), 1);
}

#[test]
fn malformed_visibility_is_not_replaced() {
    let fixture = Fixture::new();
    let (repository, worktree) = fixture.first_tree();
    let invalid = vec![
        b"broken".to_vec(),
        Vec::new(),
        vec![b' '; 1024 * 1024 + 1],
        br#"{"version":2,"hidden":[]}"#.to_vec(),
        br#"{"version":1,"hidden":[{"repository":"relative","worktree":"/absolute"}]}"#.to_vec(),
        br#"{"version":1,"hidden":[{"repository":"/absolute","worktree":"relative"}]}"#.to_vec(),
        serde_json::to_vec(&serde_json::json!({ "version": 1, "hidden": [
            { "repository": repository, "worktree": worktree },
            { "repository": repository, "worktree": worktree }
        ] })).unwrap(),
    ];
    for bytes in invalid {
        fs::write(fixture.path(), &bytes).unwrap();
        assert!(Visibility::read(&fixture.path()).is_err());
        assert!(Visibility::persist_toggle(&fixture.path(), repository, worktree).is_err());
        assert_eq!(fs::read(fixture.path()).unwrap(), bytes);
    }
}

#[test]
fn lock_contention_is_nonblocking_and_preserves_last_good_visibility() {
    let fixture = Fixture::new();
    let (repository, worktree) = fixture.first_tree();
    let saved = Visibility::persist_toggle(&fixture.path(), repository, worktree).unwrap();
    let bytes = fs::read(fixture.path()).unwrap();
    let lock = fs::OpenOptions::new().read(true).write(true).open(fixture.path().with_extension("lock")).unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    let start = Instant::now();
    assert!(Visibility::persist_toggle(&fixture.path(), repository, &fixture.snapshot.repos[0].worktrees.as_ref().unwrap()[1].path).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(fs::read(fixture.path()).unwrap(), bytes);
    assert_eq!(Visibility::read(&fixture.path()).unwrap(), saved);
    drop(lock);
}

#[test]
fn filtering_show_all_unhide_and_ordering_preserve_discovery_health_and_counts() {
    let fixture = Fixture::new();
    let repository = fixture.snapshot.repos[0].common_dir.clone();
    let trees = fixture.snapshot.repos[0].worktrees.as_ref().unwrap();
    let movement = OrderMove::Worktree { repository: repository.clone(), worktree: trees[1].path.clone(), before: Some(trees[0].path.clone()) };
    let order = Order::default().moved(&fixture.snapshot, &movement).unwrap().unwrap();
    let mut ordered = fixture.snapshot.clone();
    order.apply(&mut ordered);
    let visibility = Visibility::persist_toggle(&fixture.path(), &repository, &trees[0].path).unwrap();
    let filtered = DashboardView::from_snapshot_with_visibility(&ordered, &visibility, false);
    assert_eq!(filtered.health, Health::Errors);
    assert_eq!(filtered.items[0].display.caption(), "alpha");
    assert_eq!(worktree_keys(&filtered).len(), 3);
    assert_eq!(worktree_keys(&filtered)[0], &ItemKey::Worktree(identity(&repository), identity(&trees[1].path)));
    let all = DashboardView::from_snapshot_with_visibility(&ordered, &visibility, true);
    assert_eq!(worktree_keys(&all), vec![
        &ItemKey::Worktree(identity(&repository), identity(&trees[1].path)),
        &ItemKey::Worktree(identity(&repository), identity(&trees[0].path)),
        &ItemKey::Worktree(identity(&fixture.snapshot.repos[1].common_dir), identity(&fixture.snapshot.repos[1].worktrees.as_ref().unwrap()[0].path)),
        &ItemKey::Worktree(identity(&fixture.snapshot.repos[1].common_dir), identity(&fixture.snapshot.repos[1].worktrees.as_ref().unwrap()[1].path)),
    ]);
    let visibility = Visibility::persist_toggle(&fixture.path(), &repository, &trees[0].path).unwrap();
    let unhidden = DashboardView::from_snapshot_with_visibility(&ordered, &visibility, false);
    assert_eq!(worktree_keys(&unhidden), worktree_keys(&all));
}

#[test]
fn hiding_the_only_failed_worktree_does_not_clear_error_health() {
    let mut fixture = Fixture::new();
    fixture.snapshot.repos.truncate(1);
    let repository = fixture.snapshot.repos[0].common_dir.clone();
    let trees = fixture.snapshot.repos[0].worktrees.as_mut().unwrap();
    trees[1].result = Ok(WorktreeStats { head: Head::Branch("topic".into()), counts: Counts::default() });
    let hidden = trees[0].path.clone();
    let visible = trees[1].path.clone();
    let visibility = Visibility::persist_toggle(&fixture.path(), &repository, &hidden).unwrap();
    let filtered = DashboardView::from_snapshot_with_visibility(&fixture.snapshot, &visibility, false);
    assert_eq!(filtered.health, Health::Errors);
    assert_eq!(worktree_keys(&filtered), vec![&ItemKey::Worktree(identity(&repository), identity(&visible))]);
    assert_eq!(filtered.items[0].display.caption(), "alpha");
    let all = DashboardView::from_snapshot_with_visibility(&fixture.snapshot, &visibility, true);
    assert_eq!(all.health, Health::Errors);
    assert_eq!(worktree_keys(&all), vec![
        &ItemKey::Worktree(identity(&repository), identity(&hidden)),
        &ItemKey::Worktree(identity(&repository), identity(&visible)),
    ]);
}

#[cfg(windows)]
#[test]
fn saved_visibility_matches_windows_path_aliases() {
    let fixture = Fixture::new();
    let repository = Path::new(r"C:\Projects\Example\.git");
    let worktree = Path::new(r"C:\Projects\Example-Topic");
    Visibility::persist_toggle(&fixture.path(), repository, worktree).unwrap();
    let restored = Visibility::read(&fixture.path()).unwrap();
    assert!(restored.contains(Path::new(r"\\?\C:\PROJECTS\EXAMPLE\.GIT"), Path::new("c:/projects/example-topic")));
    Visibility::persist_toggle(&fixture.path(), Path::new(r"\\server\share\repo.git"), Path::new(r"\\server\share\topic")).unwrap();
    let restored = Visibility::read(&fixture.path()).unwrap();
    assert!(restored.contains(Path::new(r"\\?\UNC\SERVER\SHARE\REPO.GIT"), Path::new(r"\\SERVER\SHARE\TOPIC")));
    assert_eq!(restored.hidden().len(), 2);
}

#[cfg(windows)]
#[test]
fn failed_atomic_replacement_preserves_saved_visibility() {
    let fixture = Fixture::new();
    let (repository, worktree) = fixture.first_tree();
    let saved = Visibility::persist_toggle(&fixture.path(), repository, worktree).unwrap();
    let bytes = fs::read(fixture.path()).unwrap();
    let original_permissions = fs::metadata(fixture.path()).unwrap().permissions();
    let mut permissions = original_permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(fixture.path(), permissions).unwrap();
    let result = Visibility::persist_toggle(&fixture.path(), repository, worktree);
    fs::set_permissions(fixture.path(), original_permissions).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(fixture.path()).unwrap(), bytes);
    assert_eq!(Visibility::read(&fixture.path()).unwrap(), saved);
}
