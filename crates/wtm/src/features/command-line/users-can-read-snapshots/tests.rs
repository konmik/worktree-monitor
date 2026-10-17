use crate::invoke;
use std::fs;
use std::process::Command;

#[test]
fn default_configuration_path_empty_and_malformed_are_visible() {
    let parent = std::env::temp_dir().join("opencode");
    fs::create_dir_all(&parent).unwrap();
    let home = tempfile::tempdir_in(parent).unwrap();
    let empty = invoke(&[], home.path());
    assert_eq!(empty.status.code(), Some(0));
    assert_eq!(empty.stdout, b"No registered repositories.\n");
    let config = home.path().join(".config/wtm/config.json");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, b"broken").unwrap();
    let malformed = invoke(&[], home.path());
    assert_eq!(malformed.status.code(), Some(1));
    assert!(String::from_utf8(malformed.stdout).unwrap().contains("Configuration error: Invalid configuration:"));
    assert_eq!(fs::read(config).unwrap(), b"broken");
}

#[test]
fn normal_cli_does_not_include_launch_repository_or_read_presentation_preferences() {
    let parent = std::env::temp_dir().join("opencode");
    fs::create_dir_all(&parent).unwrap();
    let fixture = tempfile::tempdir_in(parent).unwrap();
    let repo = fixture.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let initialized = Command::new("git").arg("-C").arg(&repo).args(["init", "--quiet"])
        .env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").env_remove("GIT_INDEX_FILE").env_remove("GIT_COMMON_DIR")
        .output().unwrap();
    assert!(initialized.status.success());
    let home = fixture.path().join("home");
    let config = home.join(".config/wtm/config.json");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(config.with_extension("order.json"), b"broken").unwrap();
    let snapshot = Command::new(env!("CARGO_BIN_EXE_wtm")).current_dir(repo)
        .env("HOME", &home).env("USERPROFILE", &home).output().unwrap();
    assert!(snapshot.status.success());
    assert_eq!(snapshot.stdout, b"No registered repositories.\n");
    assert_eq!(snapshot.stderr, b"");
    assert!(!config.exists());
    assert_eq!(fs::read(config.with_extension("order.json")).unwrap(), b"broken");
}
