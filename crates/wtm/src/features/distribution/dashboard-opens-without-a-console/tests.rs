#[cfg(windows)]
#[test]
fn executables_have_their_intended_windows_subsystems() {
    for (executable, expected) in [(env!("CARGO_BIN_EXE_wtm"), 3), (env!("CARGO_BIN_EXE_wtm-tui"), 3), (env!("CARGO_BIN_EXE_wtm-gui"), 2)] {
        let image = std::fs::read(executable).unwrap();
        assert_eq!(&image[..2], b"MZ");
        let pe = u32::from_le_bytes(image[0x3c..0x40].try_into().unwrap()) as usize;
        assert_eq!(&image[pe..pe + 4], b"PE\0\0");
        let subsystem = pe + 24 + 68;
        assert_eq!(u16::from_le_bytes(image[subsystem..subsystem + 2].try_into().unwrap()), expected, "{executable}");
    }
}

#[cfg(windows)]
#[test]
fn deployment_installs_executables_without_command_wrappers() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    let script = root.join("scripts/deploy.ps1");
    let source = std::fs::read_to_string(&script).unwrap();
    assert!(!source.contains(".cmd"), "scripts/deploy.ps1 must use cargo install directly; command wrappers open a console for wtm-gui");
    let directory = tempfile::tempdir().unwrap();
    for exit_code in [0, 42] {
        let output = std::process::Command::new("pwsh")
            .args(["-NoProfile", "-NonInteractive", "-Command", r#"
function cargo {
    [pscustomobject]@{ arguments = @($args); directory = (Get-Location).Path } | ConvertTo-Json -Compress
    $global:LASTEXITCODE = [int]$env:WTM_TEST_CARGO_EXIT
}
& $env:WTM_DEPLOY_SCRIPT
"#])
            .current_dir(directory.path())
            .env("WTM_DEPLOY_SCRIPT", &script)
            .env("WTM_TEST_CARGO_EXIT", exit_code.to_string())
            .env("USERPROFILE", directory.path())
            .env("LOCALAPPDATA", directory.path())
            .output().unwrap();
        let invocation: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(invocation["arguments"], serde_json::json!(["install", "--path", "crates/wtm", "--locked", "--bins"]));
        assert_eq!(dunce::canonicalize(invocation["directory"].as_str().unwrap()).unwrap(), dunce::canonicalize(root).unwrap());
        assert_eq!(output.status.success(), exit_code == 0, "{}", String::from_utf8_lossy(&output.stderr));
        assert!(!directory.path().join("bin").exists(), "Deployment must leave executable installation to Cargo instead of creating command aliases");
    }
}
