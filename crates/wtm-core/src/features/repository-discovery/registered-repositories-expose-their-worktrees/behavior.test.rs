#[test]
fn missing_repository_is_error_not_zero_and_can_be_removed_by_old_root() {
    let fixture = Fixture::new("sha1");
    fixture.service().add(&fixture.repo).unwrap();
    fs::rename(&fixture.repo, fixture.repo.with_file_name("moved")).unwrap();
    let snapshot = fixture.service().snapshot(&Cancel::new());
    assert!(snapshot.has_errors());
    assert!(snapshot.repos[0].worktrees.is_err());
    assert!(!snapshot.text().contains("S +0/-0"));
    assert!(fixture.service().remove(&fixture.repo).unwrap());
    assert_eq!(fixture.service().snapshot(&Cancel::new()).text(), "No registered repositories.\n");
}

#[test]
fn bare_repository_is_registered_without_a_fake_worktree() {
    let fixture = Fixture::new("sha1");
    let bare = fixture._directory.path().join("bare.git");
    fs::create_dir(&bare).unwrap();
    git(&bare, &["init", "--bare", "--quiet"]);
    fixture.service().add(&bare).unwrap();
    let snapshot = fixture.service().snapshot(&Cancel::new());
    assert_eq!(snapshot.repos[0].worktrees, Ok(Vec::new()));
    assert!(!snapshot.has_errors());
    assert!(snapshot.text().contains("No working trees (bare repository)."));
}
