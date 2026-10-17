use crate::invoke;
use std::fs;
use std::process::Command;

#[test]
fn registration_clears_inherited_git_repository_overrides() {
    let parent = std::env::temp_dir().join("opencode");
    fs::create_dir_all(&parent).unwrap();
    let fixture = tempfile::tempdir_in(parent).unwrap();
    let repo = fixture.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let initialized = Command::new("git").arg("-C").arg(&repo).args(["init", "--quiet"])
        .env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").env_remove("GIT_INDEX_FILE").env_remove("GIT_COMMON_DIR")
        .output().unwrap();
    assert_eq!(initialized.status.code(), Some(0));
    let home = fixture.path().join("home");
    let added = Command::new(env!("CARGO_BIN_EXE_wtm")).arg("add").arg(&repo)
        .env("HOME", &home).env("USERPROFILE", &home)
        .env("GIT_DIR", "not-a-git-directory").env("GIT_WORK_TREE", "missing-worktree")
        .env("GIT_INDEX_FILE", "missing-index").env("GIT_COMMON_DIR", "missing-common-directory")
        .env("GIT_CONFIG_COUNT", "not-a-count").env("GIT_CONFIG_PARAMETERS", "not-valid")
        .output().unwrap();
    assert_eq!(added.status.code(), Some(0));
    assert_eq!(added.stdout, b"Repository registered.\n");
    let stored = fs::read_to_string(home.join(".config/wtm/config.json")).unwrap();
    let common = repo.join(".git").to_string_lossy().replace('\\', "\\\\");
    assert!(stored.contains(&format!("\"{common}\"")));
    let duplicate = invoke(&["add", repo.to_str().unwrap()], &home);
    assert_eq!(duplicate.status.code(), Some(0));
    assert_eq!(duplicate.stdout, b"Repository already registered.\n");
}
