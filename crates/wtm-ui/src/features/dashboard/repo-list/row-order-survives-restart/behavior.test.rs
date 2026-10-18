use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use wtm_ui::dashboard::{DashboardView, ItemKey};
use wtm_ui::order::{identity, Order, OrderMove};
use wtm_core::{Error, ErrorKind, Head, RepoEntry, ScanTarget, Snapshot, WorktreeEntry, WorktreeStats};

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

#[cfg(windows)]
#[test]
fn windows_identity_accepts_case_separators_and_extended_spelling() {
    assert_eq!(identity(Path::new("C:\\Repo\\.git")), identity(Path::new("c:/repo/.git")));
    assert_eq!(identity(Path::new("\\\\?\\C:\\Repo\\.git")), identity(Path::new("c:/repo/.git")));
    assert_eq!(identity(Path::new("\\\\?\\UNC\\server\\Share\\Repo")), identity(Path::new("\\\\server\\share\\repo")));
}

#[path = "tests.rs"]
mod persistence_tests;
