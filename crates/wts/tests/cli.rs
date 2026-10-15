use std::fs;
use std::process::{Command, Output};

fn invoke(args: &[&str], home: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wts")).args(args).env("HOME", home).env("USERPROFILE", home).output().unwrap()
}

#[test]
fn help_version_and_invalid_usage_have_correct_exit_codes() {
    let parent = std::env::temp_dir().join("opencode");
    fs::create_dir_all(&parent).unwrap();
    let home = tempfile::tempdir_in(parent).unwrap();
    let help = invoke(&["--help"], home.path());
    assert_eq!(help.status.code(), Some(0));
    let help_text = String::from_utf8(help.stdout).unwrap();
    assert!(help_text.contains("--live"));
    assert!(!help_text.contains("--live-child"));
    assert_eq!(help.stderr, b"");
    let version = invoke(&["--version"], home.path());
    assert_eq!(version.status.code(), Some(0));
    assert_eq!(version.stdout, b"wts 0.1.0\n");
    for args in [&["add"][..], &["remove"][..], &["--unknown"][..], &["--live", "add", "."][..], &["--live-child"][..], &["--live", "--live-child"][..], &["--live", "--live-child", "add", "."][..], &["unexpected"][..]] {
        let result = invoke(args, home.path());
        assert_eq!(result.status.code(), Some(2));
        assert!(String::from_utf8(result.stderr).unwrap().contains("error:"));
    }
}

#[cfg(windows)]
#[test]
fn live_reports_missing_sibling_without_hanging() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let parent = std::env::temp_dir().join("opencode");
    fs::create_dir_all(&parent).unwrap();
    let fixture = tempfile::tempdir_in(parent).unwrap();
    let executable = fixture.path().join("wts.exe");
    fs::copy(env!("CARGO_BIN_EXE_wts"), &executable).unwrap();
    let mut child = Command::new(executable).arg("--live").current_dir(fixture.path())
        .env("HOME", fixture.path()).env("USERPROFILE", fixture.path())
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("wts --live did not exit when its UI executable was missing");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let result = child.wait_with_output().unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(result.stdout, b"");
    let error = String::from_utf8(result.stderr).unwrap();
    assert!(error.starts_with("Error: Could not launch "), "{error}");
    assert!(error.contains("wts-ui.exe"), "{error}");
}

#[test]
fn default_configuration_path_empty_and_malformed_are_visible() {
    let parent = std::env::temp_dir().join("opencode");
    fs::create_dir_all(&parent).unwrap();
    let home = tempfile::tempdir_in(parent).unwrap();
    let empty = invoke(&[], home.path());
    assert_eq!(empty.status.code(), Some(0));
    assert_eq!(empty.stdout, b"No registered repositories.\n");
    let config = home.path().join(".config/wts/config.json");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, b"broken").unwrap();
    let malformed = invoke(&[], home.path());
    assert_eq!(malformed.status.code(), Some(1));
    assert!(String::from_utf8(malformed.stdout).unwrap().contains("Configuration error: Invalid configuration:"));
    assert_eq!(fs::read(config).unwrap(), b"broken");
}

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
    let added = Command::new(env!("CARGO_BIN_EXE_wts")).arg("add").arg(&repo)
        .env("HOME", &home).env("USERPROFILE", &home)
        .env("GIT_DIR", "not-a-git-directory").env("GIT_WORK_TREE", "missing-worktree")
        .env("GIT_INDEX_FILE", "missing-index").env("GIT_COMMON_DIR", "missing-common-directory")
        .env("GIT_CONFIG_COUNT", "not-a-count").env("GIT_CONFIG_PARAMETERS", "not-valid")
        .output().unwrap();
    assert_eq!(added.status.code(), Some(0));
    assert_eq!(added.stdout, b"Repository registered.\n");
    let snapshot = invoke(&[], &home);
    assert_eq!(snapshot.status.code(), Some(0));
    assert!(String::from_utf8(snapshot.stdout).unwrap().contains("S +0/-0"));
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
    let config = home.join(".config/wts/config.json");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(config.with_extension("order.json"), b"broken").unwrap();
    let snapshot = Command::new(env!("CARGO_BIN_EXE_wts")).current_dir(repo)
        .env("HOME", &home).env("USERPROFILE", &home).output().unwrap();
    assert!(snapshot.status.success());
    assert_eq!(snapshot.stdout, b"No registered repositories.\n");
    assert_eq!(snapshot.stderr, b"");
    assert!(!config.exists());
    assert_eq!(fs::read(config.with_extension("order.json")).unwrap(), b"broken");
}

#[cfg(not(windows))]
#[test]
fn live_is_explicitly_unsupported_off_windows() {
    let result = invoke(&["--live"], &std::env::temp_dir());
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(result.stderr, b"Error: --live is supported only on Windows\n");
}
