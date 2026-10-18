use super::acquire_named;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

fn name() -> String {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    format!("Local\\wtm-gui-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
}

fn start_child(name: &str) -> Child {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["child_instance", "--nocapture"])
        .env("WTM_TEST_INSTANCE_NAME", name)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        assert_ne!(output.read_line(&mut line).unwrap(), 0, "child exited before acquiring the instance");
        if line.trim() == "instance acquired" {
            child.stdout = Some(output.into_inner());
            return child;
        }
        line.clear();
    }
}

#[test]
fn child_instance() {
    let Ok(name) = std::env::var("WTM_TEST_INSTANCE_NAME") else {
        return;
    };
    let _guard = acquire_named(&name).unwrap().unwrap();
    println!("instance acquired");
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
}

#[test]
fn second_instance_is_rejected_until_the_first_exits() {
    let name = name();
    let first = acquire_named(&name).unwrap().unwrap();
    assert!(acquire_named(&name).unwrap().is_none());
    drop(first);
    assert!(acquire_named(&name).unwrap().is_some());
}

#[test]
fn another_process_blocks_startup_until_it_exits() {
    let name = name();
    let mut child = start_child(&name);
    let blocked = acquire_named(&name).unwrap().is_none();
    drop(child.stdin.take());
    assert!(child.wait().unwrap().success());
    assert!(blocked);
    assert!(acquire_named(&name).unwrap().is_some());
}

#[test]
fn startup_is_available_after_the_first_process_is_terminated() {
    let name = name();
    let mut child = start_child(&name);
    let blocked = acquire_named(&name).unwrap().is_none();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(blocked);
    assert!(acquire_named(&name).unwrap().is_some());
}
