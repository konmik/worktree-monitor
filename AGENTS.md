- Keep tests in separate files from source code.
- Use `native.rs` only for direct operating-system integration. Name other source files by their responsibility.

# Idle percentage

- At each refresh, calculate the idle percentage from the 60 seconds immediately before that refresh. Recalculate at least once per second and display whole percentages.
- Completed reading time remains counted until it becomes older than 60 seconds. The displayed percentage can therefore stay unchanged while the app is idle. This is expected behavior, not a refresh bug.
- Idle rises as earlier reading time becomes older than 60 seconds. Do not add smoothing or force the number to increase on every refresh.

# Local deployment

Deploy all three release executables, `wtm`, `wtm-tui`, and `wtm-gui`, from the repository root:

```powershell
cargo install --path crates/wtm --locked --bins
```
