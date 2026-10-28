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
    assert!(!source.contains(".cmd"), "scripts/deploy.ps1 must create executable links; command wrappers open a console for wtm-gui");
    assert!(source.contains("[switch]$Worker"), "scripts/deploy.ps1 must run installation in an external worker to avoid package storage redirection");
    let directory = tempfile::tempdir_in(root.join("target")).unwrap();
    let installation = directory.path().join("local\\Programs\\WorktreeMonitor");
    let aliases = directory.path().join("user\\bin");
    for (attempt, (exit_code, package_storage)) in [(0, false), (0, false), (42, false), (0, true)].into_iter().enumerate() {
        let result_path = directory.path().join(format!("result-{attempt}.json"));
        let invocation_path = directory.path().join(format!("invocation-{attempt}.json"));
        let output = std::process::Command::new("pwsh")
            .args(["-NoProfile", "-NonInteractive", "-Command", r#"
function cargo {
    [pscustomobject]@{ arguments = @($args); directory = (Get-Location).Path } | ConvertTo-Json -Compress | Set-Content -LiteralPath $env:WTM_TEST_INVOCATION
    if ($env:WTM_TEST_CARGO_EXIT -eq '0') {
        $destination = Join-Path $args[2] 'bin'
        [void][IO.Directory]::CreateDirectory($destination)
        foreach ($name in @('wtm', 'wtm-tui', 'wtm-gui')) {
            $binary = Join-Path $destination "$name.exe"
            Copy-Item -LiteralPath $env:WTM_TEST_SOURCE -Destination "$binary.next"
            Move-Item -LiteralPath "$binary.next" -Destination $binary -Force
        }
    }
    $global:LASTEXITCODE = [int]$env:WTM_TEST_CARGO_EXIT
}
if ($env:WTM_TEST_PACKAGE_STORAGE -eq 'true') {
    function fsutil { $global:LASTEXITCODE = 0; '\Users\k\AppData\Local\Packages\OpenAI.Codex\LocalCache\Local\Programs\WorktreeMonitor\bin\wtm.exe' }
}
& $env:WTM_DEPLOY_SCRIPT -Worker -ResultPath $env:WTM_TEST_RESULT
"#])
            .current_dir(directory.path())
            .env("WTM_DEPLOY_SCRIPT", &script)
            .env("WTM_TEST_CARGO_EXIT", exit_code.to_string())
            .env("WTM_TEST_PACKAGE_STORAGE", package_storage.to_string())
            .env("WTM_TEST_RESULT", &result_path)
            .env("WTM_TEST_INVOCATION", &invocation_path)
            .env("WTM_TEST_SOURCE", env!("CARGO_BIN_EXE_wtm"))
            .env("USERPROFILE", directory.path().join("user"))
            .env("LOCALAPPDATA", directory.path().join("local"))
            .output().unwrap();
        let result_text = std::fs::read_to_string(result_path).unwrap();
        let result: serde_json::Value = serde_json::from_str(result_text.trim_start_matches('\u{feff}')).unwrap();
        let invocation: serde_json::Value = serde_json::from_slice(&std::fs::read(invocation_path).unwrap()).unwrap();
        assert_eq!(invocation["arguments"], serde_json::json!(["install", "--root", installation.to_str().unwrap(), "--path", "crates/wtm", "--locked", "--bins", "--force"]));
        assert_eq!(dunce::canonicalize(invocation["directory"].as_str().unwrap()).unwrap(), dunce::canonicalize(root).unwrap());
        let success = exit_code == 0 && !package_storage;
        assert_eq!(output.status.success(), success, "{result_text}\n{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(result["success"], success, "{result_text}");
        if package_storage {
            assert!(result["error"].as_str().unwrap().contains("outside package storage"));
            continue;
        } else if success {
            assert_eq!(result["installed"].as_array().unwrap().len(), 3);
        } else {
            assert!(result["error"].as_str().unwrap().contains("exit code 42"));
        }
        for name in ["wtm", "wtm-tui", "wtm-gui"] {
            let binary = installation.join(format!("bin/{name}.exe"));
            let alias = aliases.join(format!("{name}.exe"));
            let links = std::process::Command::new("fsutil").args(["hardlink", "list"]).arg(&binary).output().unwrap();
            assert!(links.status.success());
            let links = String::from_utf8(links.stdout).unwrap().to_lowercase();
            assert!(links.lines().any(|link| link.trim() == &alias.to_str().unwrap().to_lowercase()[2..]), "{links}");
            assert_eq!(std::fs::read(&alias).unwrap(), std::fs::read(&binary).unwrap());
        }
        assert_eq!(std::fs::read_dir(&aliases).unwrap().count(), 3);
    }
}
