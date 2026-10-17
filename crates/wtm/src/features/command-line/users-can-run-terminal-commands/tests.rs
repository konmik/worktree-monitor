use crate::invoke;
use std::fs;

#[test]
fn help_version_and_invalid_usage_have_correct_exit_codes() {
    let parent = std::env::temp_dir().join("opencode");
    fs::create_dir_all(&parent).unwrap();
    let home = tempfile::tempdir_in(parent).unwrap();
    let help = invoke(&["--help"], home.path());
    assert_eq!(help.status.code(), Some(0));
    let help_text = String::from_utf8(help.stdout).unwrap();
    assert!(!help_text.contains("--live"));
    assert_eq!(help.stderr, b"");
    let version = invoke(&["--version"], home.path());
    assert_eq!(version.status.code(), Some(0));
    assert_eq!(version.stdout, b"wtm 0.1.0\n");
    for args in [&["add"][..], &["remove"][..], &["--unknown"][..], &["--live"][..], &["unexpected"][..]] {
        let result = invoke(args, home.path());
        assert_eq!(result.status.code(), Some(2));
        assert!(String::from_utf8(result.stderr).unwrap().contains("error:"));
    }
}
