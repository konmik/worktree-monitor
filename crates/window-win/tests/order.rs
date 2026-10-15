use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use window_win::dashboard::{DashboardView, ItemKey};
use window_win::order::{identity, Order, OrderMove};
use wts_core::{Error, ErrorKind, Head, RepoEntry, ScanTarget, Snapshot, WorktreeEntry, WorktreeStats};

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

    fn path(&self) -> PathBuf { self.directory.path().join("config.order.json") }

    fn repository_move(&self) -> OrderMove {
        OrderMove::Repository { repository: self.snapshot.repos[1].common_dir.clone(), before: Some(self.snapshot.repos[0].common_dir.clone()) }
    }

    fn worktree_move(&self, index: usize) -> OrderMove {
        let repo = &self.snapshot.repos[index];
        let trees = repo.worktrees.as_ref().unwrap();
        OrderMove::Worktree { repository: repo.common_dir.clone(), worktree: trees[1].path.clone(), before: Some(trees[0].path.clone()) }
    }

    fn save(&self, movement: &OrderMove) -> Order {
        let (order, changed) = Order::persist_move(&self.path(), &self.snapshot, movement).unwrap();
        assert!(changed);
        order
    }
}

fn repo_paths(snapshot: &Snapshot) -> Vec<PathBuf> {
    snapshot.repos.iter().map(|repo| identity(&repo.common_dir)).collect()
}

fn tree_paths(snapshot: &Snapshot, repository: &Path) -> Vec<PathBuf> {
    snapshot.repos.iter().find(|repo| identity(&repo.common_dir) == identity(repository)).unwrap().worktrees.as_ref().unwrap()
        .iter().map(|tree| identity(&tree.path)).collect()
}

fn scan_targets(snapshot: &Snapshot) -> Vec<ScanTarget> {
    snapshot.repos.iter().flat_map(|repo| {
        std::iter::once(ScanTarget::Repository { common_dir: repo.common_dir.clone(), path: repo.path.clone() })
            .chain(repo.worktrees.as_ref().unwrap().iter().map(|tree| ScanTarget::Worktree {
                repository: repo.common_dir.clone(), path: tree.path.clone(),
            }))
    }).collect()
}

#[test]
fn target_order_is_stable_across_pending_later_completions_and_errors() {
    let fixture = Fixture::new();
    let repositories = Order::default().moved(&fixture.snapshot, &fixture.repository_move()).unwrap().unwrap();
    let worktrees = Order::default().moved(&fixture.snapshot, &fixture.worktree_move(0)).unwrap().unwrap();
    let saved = repositories.clone()
        .moved(&fixture.snapshot, &fixture.worktree_move(0)).unwrap().unwrap()
        .moved(&fixture.snapshot, &fixture.worktree_move(1)).unwrap().unwrap();
    let mut successful = fixture.snapshot.clone();
    for repo in &mut successful.repos {
        for tree in repo.worktrees.as_mut().unwrap() {
            tree.result = Ok(WorktreeStats { head: Head::Branch("main".into()), counts: Default::default() });
        }
    }
    for order in [Order::default(), repositories, worktrees, saved] {
        let mut targets = scan_targets(&fixture.snapshot);
        order.apply_targets(&mut targets);
        let mut ordered = fixture.snapshot.clone();
        order.apply(&mut ordered);
        let expected: Vec<_> = DashboardView::from_snapshot(&ordered).items.into_iter().map(|item| item.key).collect();
        let mut later_repo = successful.clone();
        later_repo.repos.remove(0);
        later_repo.repos[0].worktrees.as_mut().unwrap().remove(0);
        let mut later_sibling = successful.clone();
        later_sibling.repos.remove(1);
        later_sibling.repos[0].worktrees.as_mut().unwrap().remove(0);
        let mut reversed = successful.clone();
        reversed.repos.reverse();
        for repo in &mut reversed.repos { repo.worktrees.as_mut().unwrap().reverse(); }
        for snapshot in [Snapshot::default(), later_repo, later_sibling, reversed, successful.clone(), fixture.snapshot.clone()] {
            let view = DashboardView::from_scan(&snapshot, &targets, &Default::default(), false);
            assert_eq!(view.items.into_iter().map(|item| item.key).collect::<Vec<_>>(), expected);
        }
    }
}

#[test]
fn target_partial_ranks_keep_unranked_groups_and_siblings_in_discovery_order() {
    let fixture = Fixture::new();
    let mut snapshot = fixture.snapshot.clone();
    snapshot.repos[0].worktrees.as_mut().unwrap().push(WorktreeEntry {
        path: fixture.directory.path().join("third-tree"), result: Err(Error { kind: ErrorKind::Git, message: "Unavailable".into() }),
    });
    let mut gamma = snapshot.repos[0].clone();
    gamma.common_dir = fixture.directory.path().join("gamma/.git");
    gamma.path = fixture.directory.path().join("gamma");
    snapshot.repos.push(gamma);
    let alpha = &snapshot.repos[0];
    let trees = alpha.worktrees.as_ref().unwrap();
    fs::write(fixture.path(), serde_json::to_vec(&serde_json::json!({ "version": 1,
        "repositories": [snapshot.repos[1].common_dir, fixture.directory.path().join("absent/.git")],
        "worktrees": [{ "repository": alpha.common_dir, "paths": [trees[1].path] }]
    })).unwrap()).unwrap();
    let mut targets = scan_targets(&snapshot);
    targets.sort_by_key(|target| matches!(target, ScanTarget::Worktree { .. }));
    for order in [Order::default(), Order::read(&fixture.path()).unwrap()] {
        let mut actual = targets.clone();
        order.apply_targets(&mut actual);
        let mut expected = snapshot.clone();
        order.apply(&mut expected);
        assert_eq!(actual, scan_targets(&expected));
        order.apply_targets(&mut actual);
        assert_eq!(actual, scan_targets(&expected));
    }
}

#[test]
fn defaults_preserve_discovery_order_and_missing_preferences_do_not_write() {
    let fixture = Fixture::new();
    let mut snapshot = fixture.snapshot.clone();
    Order::read(&fixture.path()).unwrap().apply(&mut snapshot);
    assert_eq!(snapshot, fixture.snapshot);
    assert!(!fixture.path().exists());
}

#[test]
fn repository_moves_carry_groups_and_worktree_moves_only_change_siblings() {
    let fixture = Fixture::new();
    let order = Order::default().moved(&fixture.snapshot, &fixture.repository_move()).unwrap().unwrap();
    let order = order.moved(&fixture.snapshot, &fixture.worktree_move(0)).unwrap().unwrap();
    let mut snapshot = fixture.snapshot.clone();
    order.apply(&mut snapshot);
    assert_eq!(repo_paths(&snapshot), repo_paths(&fixture.snapshot).into_iter().rev().collect::<Vec<_>>());
    let view = DashboardView::from_snapshot(&snapshot);
    let alpha = identity(&fixture.snapshot.repos[0].common_dir);
    let beta = identity(&fixture.snapshot.repos[1].common_dir);
    assert_eq!(view.items.iter().map(|item| &item.key).collect::<Vec<_>>(), vec![
        &ItemKey::Repository(beta.clone()),
        &ItemKey::Worktree(beta.clone(), identity(&fixture.snapshot.repos[1].worktrees.as_ref().unwrap()[0].path)),
        &ItemKey::Worktree(beta, identity(&fixture.snapshot.repos[1].worktrees.as_ref().unwrap()[1].path)),
        &ItemKey::Repository(alpha.clone()),
        &ItemKey::Worktree(alpha.clone(), identity(&fixture.snapshot.repos[0].worktrees.as_ref().unwrap()[1].path)),
        &ItemKey::Worktree(alpha, identity(&fixture.snapshot.repos[0].worktrees.as_ref().unwrap()[0].path)),
    ]);
    let order = order.moved(&fixture.snapshot, &OrderMove::Worktree { repository: fixture.snapshot.repos[0].common_dir.clone(),
        worktree: fixture.snapshot.repos[0].worktrees.as_ref().unwrap()[1].path.clone(), before: None }).unwrap().unwrap();
    order.apply(&mut snapshot);
    assert_eq!(tree_paths(&snapshot, &alpha_path(&fixture)), tree_paths(&fixture.snapshot, &alpha_path(&fixture)));
}

fn alpha_path(fixture: &Fixture) -> PathBuf { fixture.snapshot.repos[0].common_dir.clone() }

#[test]
fn invalid_cross_repository_and_stale_moves_are_rejected_but_failed_stats_rows_are_real() {
    let fixture = Fixture::new();
    let order = Order::default();
    assert!(order.moved(&fixture.snapshot, &fixture.worktree_move(0)).unwrap().is_some());
    let alpha = &fixture.snapshot.repos[0];
    let trees = alpha.worktrees.as_ref().unwrap();
    let movement = OrderMove::Worktree { repository: alpha.common_dir.clone(), worktree: trees[0].path.clone(),
        before: Some(fixture.snapshot.repos[1].worktrees.as_ref().unwrap()[0].path.clone()) };
    assert!(order.moved(&fixture.snapshot, &movement).is_err());
    let movement = OrderMove::Repository { repository: alpha.common_dir.clone(), before: Some(fixture.directory.path().join("absent")) };
    assert!(order.moved(&fixture.snapshot, &movement).is_err());
    let mut missing = fixture.snapshot.clone();
    missing.repos[0].worktrees = Err(Error { kind: ErrorKind::Git, message: "Listing failed".into() });
    assert!(order.moved(&missing, &fixture.worktree_move(0)).is_err());
    missing.repos.remove(1);
    assert!(order.moved(&missing, &fixture.repository_move()).is_err());
}

#[test]
fn no_op_does_not_create_or_rewrite_preferences() {
    let fixture = Fixture::new();
    let movement = OrderMove::Repository { repository: fixture.snapshot.repos[1].common_dir.clone(), before: None };
    assert!(!Order::persist_move(&fixture.path(), &fixture.snapshot, &movement).unwrap().1);
    assert!(!fixture.path().exists());
    fixture.save(&fixture.repository_move());
    let bytes = fs::read(fixture.path()).unwrap();
    assert!(!Order::persist_move(&fixture.path(), &fixture.snapshot, &fixture.repository_move()).unwrap().1);
    assert_eq!(fs::read(fixture.path()).unwrap(), bytes);
}

#[test]
fn restart_persists_scoped_moves_without_touching_registrations() {
    let fixture = Fixture::new();
    let registrations = fixture.path().with_file_name("config.json");
    fs::write(&registrations, b"{ \"repos\": [] }\n").unwrap();
    fixture.save(&fixture.repository_move());
    fixture.save(&fixture.worktree_move(0));
    fixture.save(&fixture.worktree_move(1));
    let mut restarted = fixture.snapshot.clone();
    Order::read(&fixture.path()).unwrap().apply(&mut restarted);
    assert_eq!(repo_paths(&restarted), repo_paths(&fixture.snapshot).into_iter().rev().collect::<Vec<_>>());
    for repo in &fixture.snapshot.repos {
        assert_eq!(tree_paths(&restarted, &repo.common_dir), tree_paths(&fixture.snapshot, &repo.common_dir).into_iter().rev().collect::<Vec<_>>());
    }
    assert_eq!(fs::read(registrations).unwrap(), b"{ \"repos\": [] }\n");
}

#[test]
fn saved_missing_paths_survive_moves_and_new_members_append_in_default_order() {
    let fixture = Fixture::new();
    fixture.save(&fixture.repository_move());
    fixture.save(&fixture.worktree_move(0));
    let mut absent = fixture.snapshot.clone();
    absent.repos.remove(1);
    let mut fresh = fixture.snapshot.repos[1].clone();
    fresh.common_dir = fixture.directory.path().join("new/.git");
    absent.repos.push(fresh.clone());
    let movement = OrderMove::Repository { repository: fresh.common_dir.clone(), before: Some(absent.repos[0].common_dir.clone()) };
    let order = Order::persist_move(&fixture.path(), &absent, &movement).unwrap().0;
    let mut restored = fixture.snapshot.clone();
    restored.repos.push(fresh);
    order.apply(&mut restored);
    assert_eq!(repo_paths(&restored), vec![identity(&fixture.snapshot.repos[1].common_dir),
        identity(&absent.repos[1].common_dir), identity(&fixture.snapshot.repos[0].common_dir)]);
    let mut only_alpha = fixture.snapshot.clone();
    only_alpha.repos.remove(1);
    let trees = only_alpha.repos[0].worktrees.as_mut().unwrap();
    let missing = trees.remove(1);
    trees.push(WorktreeEntry { path: fixture.directory.path().join("new-tree"), result: missing.result.clone() });
    order.apply(&mut only_alpha);
    assert_eq!(only_alpha.repos[0].worktrees.as_ref().unwrap()[0].path, fixture.snapshot.repos[0].worktrees.as_ref().unwrap()[0].path);
    only_alpha.repos[0].worktrees.as_mut().unwrap().push(missing.clone());
    order.apply(&mut only_alpha);
    assert_eq!(only_alpha.repos[0].worktrees.as_ref().unwrap()[0].path, missing.path);
}

#[test]
fn partial_ranks_deduplicate_and_cannot_discover_repositories() {
    let fixture = Fixture::new();
    let beta = &fixture.snapshot.repos[1].common_dir;
    let trees = fixture.snapshot.repos[0].worktrees.as_ref().unwrap();
    fs::write(fixture.path(), serde_json::to_vec(&serde_json::json!({ "version": 1,
        "repositories": [beta, beta, fixture.directory.path().join("stale/.git")], "worktrees": [
            { "repository": fixture.snapshot.repos[0].common_dir, "paths": [trees[1].path, trees[1].path] }
        ] })).unwrap()).unwrap();
    let mut snapshot = fixture.snapshot.clone();
    Order::read(&fixture.path()).unwrap().apply(&mut snapshot);
    assert_eq!(snapshot.repos.len(), 2);
    assert_eq!(snapshot.repos[0].common_dir, *beta);
    assert_eq!(tree_paths(&snapshot, &alpha_path(&fixture)), tree_paths(&fixture.snapshot, &alpha_path(&fixture)).into_iter().rev().collect::<Vec<_>>());
}

#[test]
fn malformed_empty_oversized_unsupported_and_relative_preferences_are_not_overwritten() {
    let fixture = Fixture::new();
    let repository = &fixture.snapshot.repos[0].common_dir;
    let invalid = vec![b"broken".to_vec(), Vec::new(), vec![b' '; 1024 * 1024 + 1],
        br#"{"version":2,"repositories":[],"worktrees":[]}"#.to_vec(),
        br#"{"version":1,"repositories":["relative"],"worktrees":[]}"#.to_vec(),
        br#"{"version":1,"repositories":[],"worktrees":[{"repository":"relative","paths":[]}]}"#.to_vec(),
        serde_json::to_vec(&serde_json::json!({ "version": 1, "repositories": [], "worktrees": [
            { "repository": repository, "paths": ["relative"] }
        ] })).unwrap(),
        serde_json::to_vec(&serde_json::json!({ "version": 1, "repositories": [], "worktrees": [
            { "repository": repository, "paths": [] }, { "repository": repository, "paths": [] }
        ] })).unwrap()];
    for bytes in invalid {
        fs::write(fixture.path(), &bytes).unwrap();
        assert!(Order::read(&fixture.path()).is_err());
        assert!(Order::persist_move(&fixture.path(), &fixture.snapshot, &fixture.repository_move()).is_err());
        assert_eq!(fs::read(fixture.path()).unwrap(), bytes);
    }
}

#[test]
fn lock_contention_is_nonblocking_and_preserves_the_previous_file() {
    let fixture = Fixture::new();
    let order = fixture.save(&fixture.repository_move());
    let bytes = fs::read(fixture.path()).unwrap();
    let lock = fs::OpenOptions::new().read(true).write(true).open(fixture.path().with_extension("lock")).unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    let start = Instant::now();
    assert!(Order::persist_move(&fixture.path(), &fixture.snapshot, &fixture.worktree_move(0)).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(fs::read(fixture.path()).unwrap(), bytes);
    assert_eq!(Order::read(&fixture.path()).unwrap(), order);
    drop(lock);
    fixture.save(&fixture.worktree_move(0));
}

#[test]
fn a_directory_destination_is_rejected_without_replacing_it() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.path()).unwrap();
    assert!(Order::persist_move(&fixture.path(), &fixture.snapshot, &fixture.repository_move()).is_err());
    assert!(fixture.path().is_dir());
}

#[cfg(windows)]
#[test]
fn failed_atomic_replacement_preserves_the_saved_order() {
    let fixture = Fixture::new();
    let order = fixture.save(&fixture.repository_move());
    let bytes = fs::read(fixture.path()).unwrap();
    let original_permissions = fs::metadata(fixture.path()).unwrap().permissions();
    let mut permissions = original_permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(fixture.path(), permissions).unwrap();
    let result = Order::persist_move(&fixture.path(), &fixture.snapshot, &fixture.worktree_move(0));
    fs::set_permissions(fixture.path(), original_permissions).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(fixture.path()).unwrap(), bytes);
    assert_eq!(Order::read(&fixture.path()).unwrap(), order);
}

#[cfg(windows)]
#[test]
fn windows_identity_accepts_case_separators_and_extended_spelling() {
    assert_eq!(identity(Path::new("C:\\Repo\\.git")), identity(Path::new("c:/repo/.git")));
    assert_eq!(identity(Path::new("\\\\?\\C:\\Repo\\.git")), identity(Path::new("c:/repo/.git")));
    assert_eq!(identity(Path::new("\\\\?\\UNC\\server\\Share\\Repo")), identity(Path::new("\\\\server\\share\\repo")));
}
