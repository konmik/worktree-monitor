#[test]
fn malformed_registry_is_preserved_by_add_and_remove() {
    let fixture = Fixture::new("sha1");
    fs::create_dir_all(fixture.config.parent().unwrap()).unwrap();
    fs::write(&fixture.config, b"{invalid}").unwrap();
    assert_eq!(fixture.register(&fixture.repo).unwrap_err().kind, ErrorKind::Config);
    assert_eq!(fixture.remove(&fixture.repo).unwrap_err().kind, ErrorKind::Config);
    assert_eq!(fs::read(&fixture.config).unwrap(), b"{invalid}");
    assert!(fixture.service().snapshot(&Cancel::new()).text().contains("Configuration error:"));
    fs::write(&fixture.config, b"").unwrap();
    assert_eq!(fixture.register(&fixture.repo).unwrap_err().kind, ErrorKind::Config);
    assert_eq!(fs::read(&fixture.config).unwrap(), b"");
}

#[test]
fn contended_configuration_lock_times_out_without_overwriting() {
    let first = Fixture::new("sha1");
    let second = Fixture::new("sha1");
    first.register(&first.repo).unwrap();
    let original = fs::read(&first.config).unwrap();
    let lock = fs::OpenOptions::new().read(true).write(true).open(first.config.with_extension("lock")).unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    let start = Instant::now();
    let result = first.register(&second.repo);
    assert_eq!(result.unwrap_err().kind, ErrorKind::Timeout);
    assert!(start.elapsed() < Duration::from_secs(6));
    assert_eq!(fs::read(&first.config).unwrap(), original);
    drop(lock);
    assert!(first.register(&second.repo).unwrap());
    assert_eq!(first.service().snapshot(&Cancel::new()).repos.len(), 2);
}

#[test]
fn repeated_registration_and_removal_are_idempotent() {
    let fixture = Fixture::new("sha1");
    let subdirectory = fixture.repo.join("nested");
    fs::create_dir(&subdirectory).unwrap();
    assert!(fixture.register(&subdirectory).unwrap());
    assert!(!fixture.register(&fixture.repo).unwrap());
    let json: serde_json::Value = serde_json::from_slice(&fs::read(&fixture.config).unwrap()).unwrap();
    assert_eq!(json["repos"].as_array().unwrap().len(), 1);
    assert_eq!(PathBuf::from(json["repos"][0].as_str().unwrap()), dunce::canonicalize(fixture.repo.join(".git")).unwrap());
    assert!(fixture.remove(&fixture.repo).unwrap());
    assert!(!fixture.remove(&fixture.repo).unwrap());
    assert_eq!(fixture.service().snapshot(&Cancel::new()).text(), "No registered repositories.\n");
}

#[test]
fn concurrent_distinct_writers_preserve_all_registrations() {
    let first = Fixture::new("sha1");
    let second = Fixture::new("sha1");
    let barrier = Arc::new(Barrier::new(3));
    let threads: Vec<_> = [&first.repo, &second.repo].into_iter().map(|repo| {
        let repo = repo.clone();
        let service = first.service();
        let barrier = barrier.clone();
        thread::spawn(move || { barrier.wait(); registration::update(service.config_path(), &repo, true, &Cancel::new()).unwrap(); })
    }).collect();
    barrier.wait();
    for thread in threads { thread.join().unwrap(); }
    let json: serde_json::Value = serde_json::from_slice(&fs::read(&first.config).unwrap()).unwrap();
    assert_eq!(json["repos"].as_array().unwrap().len(), 2);
    assert_eq!(first.service().snapshot(&Cancel::new()).repos.len(), 2);
}

#[test]
fn multiple_registrations_remain_idempotent_with_mixed_path_order() {
    let fixture = Fixture::new("sha1");
    let second = fixture.repo.with_file_name("Z");
    let unregistered = fixture.repo.with_file_name("unregistered");
    for repo in [&second, &unregistered] {
        fs::create_dir(repo).unwrap();
        git(repo, &["init", "--quiet"]);
    }
    let service = fixture.service();
    assert!(registration::update(service.config_path(), &fixture.repo, true, &Cancel::new()).unwrap());
    assert!(registration::update(service.config_path(), &second, true, &Cancel::new()).unwrap());
    assert_eq!(service.snapshot(&Cancel::new()).repos.len(), 2);
    let original = fs::read(&fixture.config).unwrap();
    assert!(!registration::update(service.config_path(), &fixture.repo, true, &Cancel::new()).unwrap());
    assert_eq!(fs::read(&fixture.config).unwrap(), original);
    assert!(!registration::update(service.config_path(), &unregistered, false, &Cancel::new()).unwrap());
    assert_eq!(fs::read(&fixture.config).unwrap(), original);
    assert_eq!(service.snapshot(&Cancel::new()).repos.len(), 2);
}
