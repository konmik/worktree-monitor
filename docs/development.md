# Testing and releases

Build and run the Rust checks on Windows x64 with the MSVC Rust toolchain, Visual Studio C++ build tools, a Windows SDK, and Git on `PATH`:

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --locked -p wtm --bins
pwsh -NoProfile -File scripts/verification-directories.tests.ps1
```

## Desktop checks

The smoke scripts require PowerShell 7. Run desktop checks one at a time on an unlocked interactive Windows desktop. They create isolated Git fixtures and configuration, launch test windows, and exercise the keyboard and mouse. Verification repositories use fresh Git-ignored directories under `target/verification` and are removed after success or failure. Screenshots and verification results remain in separate evidence directories.

```powershell
./scripts/smoke-cli.ps1 -Binary ./target/release/wtm.exe
./scripts/smoke-ui.ps1 -Binary ./target/release/wtm-gui.exe
./scripts/smoke-window.ps1 -Binary ./target/release/wtm-gui.exe
./scripts/smoke-dashboard.ps1 -Binary ./target/release/wtm-gui.exe
./scripts/smoke-features.ps1 -Binary ./target/release/wtm-gui.exe
./scripts/smoke-frame.ps1 -Binary ./target/release/wtm-gui.exe
./scripts/smoke-resize.ps1 -Binary ./target/release/wtm-gui.exe
./scripts/smoke-visibility.ps1 -Binary ./target/release/wtm-gui.exe
./scripts/smoke-opacity.ps1 -Binary ./target/release/wtm-gui.exe
./scripts/smoke-indicator-opacity.ps1 -Binary ./target/release/wtm-gui.exe -CliBinary ./target/release/wtm.exe
./scripts/smoke-placement.ps1 -Binary ./target/release/wtm-gui.exe
./scripts/smoke-launch.ps1 -Binary ./target/release/wtm-gui.exe
./scripts/smoke-tray-drag.ps1 -Binary ./target/release/wtm-gui.exe
./scripts/smoke-refreshing.ps1 -Binary ./target/release/wtm-gui.exe
./scripts/smoke-refreshing.ps1 -Binary ./target/release/wtm-gui.exe -SavedOrder
./scripts/capture-window.ps1 -Binary ./target/release/wtm-gui.exe -Output ./target/dashboard.png -VerifyScrollbar -VerifySerialGit
cargo build --release --locked -p wtm-ui --example measure-monitor
./scripts/check-monitor-passes.ps1
```

The spacing check inspects a running dashboard that already has adjacent worktrees:

```powershell
./scripts/smoke-worktree-spacing.ps1 -ProcessId <dashboard-process-id>
```

## Release package

Python 3 generates `target/THIRD-PARTY-NOTICES.txt` from the locked Windows dependency graph. The notices include the dependencies' license, copyright, notice, and patent files. Generated notices are included in release ZIPs.

```powershell
python ./scripts/generate-third-party-notices.py
./scripts/package-release.ps1
```

The package in `target/dist` contains all three Windows x64 executables, usage instructions, the MIT license, and third-party notices. A separate SHA-256 file identifies the ZIP.

Set the package version before creating its matching release tag. Upload the ZIP and SHA-256 file when publishing a GitHub Release.

Sources: [Rust setup on Windows](https://learn.microsoft.com/en-us/windows/dev-environment/rust/setup), [GitHub releases](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository).
