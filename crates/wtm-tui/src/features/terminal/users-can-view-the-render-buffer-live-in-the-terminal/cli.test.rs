use std::fs;
use std::process::Command;

#[test]
fn live_rejects_redirected_input_and_output_before_reading_configuration() {
    let parent = std::env::temp_dir().join("opencode");
    fs::create_dir_all(&parent).unwrap();
    let home = tempfile::tempdir_in(parent).unwrap();
    let config = home.path().join(".config/wtm/config.json");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, b"broken").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_wtm-tui"))
        .env("HOME", home.path()).env("USERPROFILE", home.path()).output().unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(result.stdout, b"");
    assert!(String::from_utf8(result.stderr).unwrap().contains("requires an interactive terminal"));
    assert_eq!(fs::read(&config).unwrap(), b"broken");
    assert!(!config.with_extension("lock").exists());
}
