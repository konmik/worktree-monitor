#[test]
fn scans_do_not_write_the_index_config_or_git_objects() {
    let fixture = Fixture::new("sha256");
    fixture.write("staged.txt", "staged\n");
    git(&fixture.repo, &["add", "staged.txt"]);
    fixture.service().add(&fixture.repo).unwrap();
    fixture.write("untracked-one.txt", "one\n");
    fixture.write("untracked-two.txt", "two\n");
    let index = fs::read(fixture.repo.join(".git/index")).unwrap();
    let config = fs::read(&fixture.config).unwrap();
    let before_objects = git(&fixture.repo, &["count-objects", "-v"]);
    assert_eq!(fixture.stats().counts, source_counts(3, 0, 0));
    assert_eq!(fs::read(fixture.repo.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(&fixture.config).unwrap(), config);
    assert_eq!(git(&fixture.repo, &["count-objects", "-v"]), before_objects);
}

#[test]
fn monitoring_does_not_execute_fsmonitor_helpers() {
    let fixture = Fixture::new("sha1");
    fixture.write("tracked.txt", "before\n");
    fixture.commit();
    fixture.service().add(&fixture.repo).unwrap();
    let hook = fixture.repo.join(".git/fsmonitor-hook");
    let marker = fixture.repo.join(".git/fsmonitor-called");
    fs::write(&hook, b"#!/bin/sh\nprintf 'called\\n' >> .git/fsmonitor-called\nprintf 'wtm-token\\0/\\0'\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(&fixture.repo, &["config", "core.fsmonitor", ".git/fsmonitor-hook"]);
    git(&fixture.repo, &["config", "core.fsmonitorHookVersion", "2"]);
    fixture.write("tracked.txt", "before\nafter\n");
    git(&fixture.repo, &["diff", "--numstat", "HEAD", "--"]);
    assert!(fs::read_to_string(&marker).unwrap().starts_with("called\n"));
    fs::remove_file(&marker).unwrap();
    assert_eq!(fixture.stats().counts, source_counts(1, 0, 0));
    assert!(!marker.exists());
}
