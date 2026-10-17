- Keep tests in separate files from source code.
- Use `native.rs` only for direct operating-system integration. Name other source files by their responsibility.

# Local deployment

Deploy both release executables, `wtm` and `wtm-ui`, from the repository root:

```powershell
cargo install --path crates/wtm --locked --bins
```
