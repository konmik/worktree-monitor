use std::process::{Command, Output};

fn invoke(args: &[&str], home: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wtm")).args(args).env("HOME", home).env("USERPROFILE", home).output().unwrap()
}

#[path = "../src/features/command-line/users-can-register-repositories/tests.rs"]
mod registration;

#[path = "../src/features/command-line/users-can-run-terminal-commands/tests.rs"]
mod commands;
