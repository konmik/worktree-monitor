- Keep tests in separate files from source code.
- Use `native.rs` only for direct operating-system integration. Name other source files by their responsibility.

# Local deployment

Deploy all three release executables, `wtm`, `wtm-tui`, and `wtm-gui`, from the repository root:

```powershell
cargo install --path crates/wtm --locked --bins
```
